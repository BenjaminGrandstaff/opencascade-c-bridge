//! Feature execution against the kernel.

use super::*;

mod holes;
mod ribs;

pub(crate) fn execute_profile_sweep<'session>(
    session: &'session Session,
    operation: &FeatureOperation,
    parameters: &HashMap<String, ParameterValue>,
    profile: &Shape<'_>,
) -> Result<Shape<'session>, ModelError> {
    let temporary_face = match session.shape_type(profile)? {
        ShapeType::Wire => Some(session.create_face_from_wire(profile)?),
        ShapeType::Face => None,
        _ => {
            return Err(ModelError::new(
                "sweep input must be a planar face or closed planar wire",
            ));
        }
    };
    let face = temporary_face.as_ref().unwrap_or(profile);
    if !session.face_is_planar(face)? || !session.is_valid(face)? {
        return Err(ModelError::new(
            "sweep profile must define a valid planar face",
        ));
    }
    let solid = match operation {
        FeatureOperation::Extrude { direction, .. } => session
            .create_prism_from_face(face, vector(direction, parameters, Dimension::Length)?)?,
        FeatureOperation::Revolve {
            origin,
            axis,
            angle_radians,
            ..
        } => session.create_revolve_from_face(
            face,
            vector(origin, parameters, Dimension::Length)?,
            vector(axis, parameters, Dimension::Scalar)?,
            scalar(angle_radians, parameters, Dimension::Scalar)?,
        )?,
        _ => unreachable!("only profile sweep operations are dispatched here"),
    };
    if session.shape_type(&solid)? != ShapeType::Solid
        || !session.is_valid(&solid)?
        || session.volume(&solid)? <= 0.0
    {
        return Err(ModelError::new(
            "profile sweep did not produce a valid solid with positive volume",
        ));
    }
    Ok(solid)
}

pub(crate) fn execute_feature<'session>(
    session: &'session Session,
    datums: &HashMap<&str, &DatumDefinition>,
    feature: &FeatureDefinition,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    let result = match &feature.operation {
        FeatureOperation::Box { origin, size } => session.create_box(
            vector(origin, parameters, Dimension::Length)?,
            vector(size, parameters, Dimension::Length)?,
        ),
        FeatureOperation::Rib {
            input,
            profile,
            thickness,
            direction,
        } => {
            return ribs::execute_rib(
                session,
                shape(shapes, input)?,
                shape(shapes, profile)?,
                scalar(thickness, parameters, Dimension::Length)?,
                vector(direction, parameters, Dimension::Scalar)?,
            );
        }
        FeatureOperation::Cylinder {
            origin,
            axis,
            radius,
            height,
        } => session.create_cylinder(
            vector(origin, parameters, Dimension::Length)?,
            vector(axis, parameters, Dimension::Scalar)?,
            scalar(radius, parameters, Dimension::Length)?,
            scalar(height, parameters, Dimension::Length)?,
        ),
        FeatureOperation::SketchFace { sketch } | FeatureOperation::SketchWire { sketch } => {
            let datum = sketch_datum(datums, &feature.operation)?
                .map(|datum| datum.kind.evaluate(parameters))
                .transpose()?;
            return if matches!(feature.operation, FeatureOperation::SketchWire { .. }) {
                sketch.wire(session, parameters, datum)
            } else {
                sketch.face_on_plane(session, parameters, datum)
            };
        }
        FeatureOperation::Translate { input, offset } => session.translate(
            shape(shapes, input)?,
            vector(offset, parameters, Dimension::Length)?,
        ),
        FeatureOperation::Extrude { input, .. } | FeatureOperation::Revolve { input, .. } => {
            return execute_profile_sweep(
                session,
                &feature.operation,
                parameters,
                shape(shapes, input)?,
            )
            .map_err(|error| error.context(&format!("profile '{input}'")));
        }
        FeatureOperation::Rotate {
            input,
            origin,
            axis,
            angle_radians,
        } => session.rotate(
            shape(shapes, input)?,
            vector(origin, parameters, Dimension::Length)?,
            vector(axis, parameters, Dimension::Scalar)?,
            scalar(angle_radians, parameters, Dimension::Scalar)?,
        ),
        FeatureOperation::Fuse { left, right } => {
            return session
                .fuse(shape(shapes, left)?, shape(shapes, right)?)
                .map_err(|error| ModelError::from(error).locate_operands([left, right]));
        }
        FeatureOperation::Cut { object, tool } => {
            return session
                .cut(shape(shapes, object)?, shape(shapes, tool)?)
                .map_err(|error| ModelError::from(error).locate_operands([object, tool]));
        }
        FeatureOperation::Hole {
            input,
            position,
            axis,
            diameter,
            extent,
            finish,
            thread,
        } => {
            return holes::execute_hole(
                session,
                shape(shapes, input)?,
                parameters,
                holes::HoleSpec {
                    position,
                    axis,
                    diameter,
                    extent,
                    finish,
                    thread: thread.as_deref(),
                },
            )
            .map_err(|error| error.context(&format!("hole input '{input}'")));
        }
        FeatureOperation::Common { left, right } => {
            return session
                .common(shape(shapes, left)?, shape(shapes, right)?)
                .map_err(|error| ModelError::from(error).locate_operands([left, right]));
        }
        FeatureOperation::Sew { inputs, tolerance } => {
            let inputs = inputs
                .iter()
                .map(|input| shape(shapes, input))
                .collect::<Result<Vec<_>, _>>()?;
            session.sew(&inputs, scalar(tolerance, parameters, Dimension::Length)?)
        }
        FeatureOperation::MakeSolid { shells } => {
            let shells = shells
                .iter()
                .map(|shell| shape(shapes, shell))
                .collect::<Result<Vec<_>, _>>()?;
            session.make_solid_from_shells(&shells)
        }
        FeatureOperation::Fillet {
            input,
            edges,
            radius,
        } => {
            return execute_fillet(
                session,
                shape(shapes, input)?,
                edges,
                scalar(radius, parameters, Dimension::Length)?,
                parameters,
                shapes,
            );
        }
        FeatureOperation::Chamfer {
            input,
            edges,
            distance,
        } => {
            return execute_chamfer(
                session,
                shape(shapes, input)?,
                edges,
                scalar(distance, parameters, Dimension::Length)?,
                parameters,
                shapes,
            );
        }
        FeatureOperation::Draft {
            input,
            faces,
            neutral_origin,
            neutral_normal,
            pull_direction,
            angle_radians,
        } => {
            let input = shape(shapes, input)?;
            let options = occt_bridge::DraftOptions {
                neutral_origin: vector(neutral_origin, parameters, Dimension::Length)?,
                neutral_normal: vector(neutral_normal, parameters, Dimension::Scalar)?,
                pull_direction: vector(pull_direction, parameters, Dimension::Scalar)?,
                angle_radians: scalar(angle_radians, parameters, Dimension::Scalar)?,
            };
            if session.subshape_count(input, ShapeType::Solid)? != 1 || !session.is_valid(input)? {
                return Err(ModelError::new("draft input must contain one valid solid"));
            }
            if faces.is_empty() {
                return Err(ModelError::new("draft requires at least one face selector"));
            }
            let mut selected = Vec::new();
            let mut sizes = Vec::new();
            for selector in faces {
                let matches = resolve_face_selector(session, input, selector, parameters, shapes)?;
                sizes.push(matches.len());
                selected.extend(matches);
            }
            let references = selected.iter().collect::<Vec<_>>();
            let result = session
                .draft(input, &references, options)
                .map_err(|error| ModelError::from(error).locate_selections(&sizes, "face"))?;
            if session.subshape_count(&result, ShapeType::Solid)? != 1
                || !session.is_valid(&result)?
                || session.volume(&result)? <= 0.0
            {
                return Err(ModelError::new(
                    "draft must leave one valid solid with positive volume",
                ));
            }
            return Ok(result);
        }
        FeatureOperation::Hollow {
            input,
            faces,
            thickness,
            tolerance,
        } => {
            return execute_hollow(
                session,
                shape(shapes, input)?,
                faces,
                scalar(thickness, parameters, Dimension::Length)?,
                scalar(tolerance, parameters, Dimension::Length)?,
                parameters,
                shapes,
            );
        }
    };
    result.map_err(Into::into)
}

pub(crate) fn execute_fillet<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[EdgeSelector],
    radius: f64,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    let (selected, sizes) =
        resolve_edge_selectors(session, input, selectors, parameters, shapes, "fillet")?;
    let references = selected.iter().collect::<Vec<_>>();
    let result = session
        .fillet(input, &references, radius)
        .map_err(|error| ModelError::from(error).locate_selections(&sizes, "edge"));
    cleanup_shapes(session, selected);
    result
}

pub(crate) fn execute_chamfer<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[EdgeSelector],
    distance: f64,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    let (selected, sizes) =
        resolve_edge_selectors(session, input, selectors, parameters, shapes, "chamfer")?;
    let references = selected.iter().collect::<Vec<_>>();
    let result = session
        .chamfer(input, &references, distance)
        .map_err(|error| ModelError::from(error).locate_selections(&sizes, "edge"));
    cleanup_shapes(session, selected);
    result
}

pub(crate) fn execute_hollow<'session>(
    session: &'session Session,
    input: &Shape<'session>,
    selectors: &[FaceSelector],
    thickness: f64,
    tolerance: f64,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    if selectors.is_empty() {
        return Err(ModelError::new(
            "hollow requires at least one face selector",
        ));
    }
    let mut selected = Vec::new();
    let mut sizes = Vec::with_capacity(selectors.len());
    for selector in selectors {
        match resolve_face_selector(session, input, selector, parameters, shapes) {
            Ok(faces) => {
                sizes.push(faces.len());
                selected.extend(faces);
            }
            Err(error) => {
                cleanup_shapes(session, selected);
                return Err(error);
            }
        }
    }
    let references = selected.iter().collect::<Vec<_>>();
    let result = session
        .hollow(input, &references, thickness, tolerance)
        .map_err(|error| ModelError::from(error).locate_selections(&sizes, "face"));
    cleanup_shapes(session, selected);
    result
}
