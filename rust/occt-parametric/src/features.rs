//! Feature execution against the kernel.

use super::*;

mod extrusions;
mod holes;
mod hollow;
mod loft;
pub(crate) use loft::collect_parameters as collect_loft_parameters;
mod primitives;
mod regions;
mod ribs;
mod sweeps;
mod variable_fillet;

pub(crate) fn execute_profile_sweep<'session>(
    session: &'session Session,
    operation: &FeatureOperation,
    parameters: &HashMap<String, ParameterValue>,
    profile: &Shape<'_>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
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
        FeatureOperation::Extrude {
            direction, extent, ..
        } => extrusions::execute(
            session,
            face,
            vector(direction, parameters, Dimension::Length)?,
            extent,
            parameters,
            shapes,
            definitions,
        )?,
        FeatureOperation::Revolve {
            origin,
            axis,
            angle_radians,
            extent,
            ..
        } => {
            let origin = vector(origin, parameters, Dimension::Length)?;
            let axis = vector(axis, parameters, Dimension::Scalar)?;
            let angle = scalar(angle_radians, parameters, Dimension::Scalar)?;
            if matches!(extent, RevolveExtent::Symmetric) {
                let placed = session.rotate(face, origin, axis, -0.5 * angle)?;
                let revolved = session.create_revolve_from_face(&placed, origin, axis, angle)?;
                session.compose_history(&revolved, &placed)?
            } else {
                session.create_revolve_from_face(face, origin, axis, angle)?
            }
        }
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
    definitions: &Features<'_>,
    feature: &FeatureDefinition,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
) -> Result<Shape<'session>, ModelError> {
    let result = match &feature.operation {
        FeatureOperation::SheetMetal { definition } => {
            return definition.generate(session, parameters);
        }
        FeatureOperation::SheetMetalFlat {
            input,
            neutral_factor,
        } => {
            let Some(FeatureDefinition {
                operation: FeatureOperation::SheetMetal { definition },
                ..
            }) = definitions.by_id.get(input.as_str()).copied()
            else {
                return Err(ModelError::new(
                    "flat pattern input must directly name a sheet-metal feature",
                ));
            };
            return definition.generate_flat(
                session,
                parameters,
                scalar(neutral_factor, parameters, Dimension::Scalar)?,
            );
        }
        FeatureOperation::Box { origin, size } => session.create_box(
            vector(origin, parameters, Dimension::Length)?,
            vector(size, parameters, Dimension::Length)?,
        ),
        FeatureOperation::Rib {
            input,
            profile,
            thickness,
            direction,
            thickness_mode,
            profile_mode,
        } => {
            return ribs::execute_rib(
                session,
                shape(shapes, input)?,
                shape(shapes, profile)?,
                scalar(thickness, parameters, Dimension::Length)?,
                vector(direction, parameters, Dimension::Scalar)?,
                *thickness_mode,
                rib_closure(profile_mode, parameters)?,
            );
        }
        FeatureOperation::ProfileLoft {
            profiles,
            holes,
            ruled,
        } => {
            return loft::execute_profiles(session, profiles, holes, *ruled, shapes);
        }
        FeatureOperation::Loft {
            sections,
            smooth,
            ruled,
        } => return loft::execute(session, sections, *smooth, *ruled, parameters),
        FeatureOperation::Sweep {
            profile,
            path,
            orientation,
        } => {
            return sweeps::execute(
                session,
                shape(shapes, profile)?,
                shape(shapes, path)?,
                match orientation {
                    SweepOrientation::CorrectedFrenet => {
                        occt_bridge::SweepOrientation::CorrectedFrenet
                    }
                    SweepOrientation::Frenet => occt_bridge::SweepOrientation::Frenet,
                    SweepOrientation::Binormal { direction } => {
                        occt_bridge::SweepOrientation::Binormal(vector(
                            direction,
                            parameters,
                            Dimension::Scalar,
                        )?)
                    }
                    SweepOrientation::Fixed => occt_bridge::SweepOrientation::Fixed,
                },
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
        FeatureOperation::Cone {
            origin,
            axis,
            base_radius,
            top_radius,
            height,
        } => {
            return primitives::cone(
                session,
                vector(origin, parameters, Dimension::Length)?,
                vector(axis, parameters, Dimension::Scalar)?,
                scalar(base_radius, parameters, Dimension::Length)?,
                scalar(top_radius, parameters, Dimension::Length)?,
                scalar(height, parameters, Dimension::Length)?,
            );
        }
        FeatureOperation::Sphere { center, radius } => session.create_sphere(
            vector(center, parameters, Dimension::Length)?,
            scalar(radius, parameters, Dimension::Length)?,
        ),
        FeatureOperation::SketchFace { sketch }
        | FeatureOperation::SketchWire { sketch }
        | FeatureOperation::SketchOpenWire { sketch } => {
            let datum = sketch_datum(datums, &feature.operation)?
                .map(|datum| datum.kind.evaluate(parameters))
                .transpose()?;
            return match feature.operation {
                FeatureOperation::SketchWire { .. } => sketch.wire(session, parameters, datum),
                FeatureOperation::SketchOpenWire { .. } => {
                    sketch.open_wire(session, parameters, datum)
                }
                _ => sketch.face_on_plane(session, parameters, datum),
            };
        }
        FeatureOperation::PlanarRegion { outer, holes } => {
            return regions::execute(session, outer, holes, shapes);
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
                shapes,
                definitions,
            )
            .map_err(|error| error.context(&format!("profile '{input}'")));
        }
        FeatureOperation::Mirror {
            input,
            origin,
            normal,
        } => session.mirror(
            shape(shapes, input)?,
            vector(origin, parameters, Dimension::Length)?,
            vector(normal, parameters, Dimension::Scalar)?,
        ),
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
        FeatureOperation::Helix {
            origin,
            axis,
            start,
            radius,
            pitch,
            turns,
            left_handed,
        } => session.create_helix_wire(occt_bridge::HelixOptions {
            origin: vector(origin, parameters, Dimension::Length)?,
            axis: vector(axis, parameters, Dimension::Scalar)?,
            start_direction: vector(start, parameters, Dimension::Scalar)?,
            radius: scalar(radius, parameters, Dimension::Length)?,
            pitch: scalar(pitch, parameters, Dimension::Length)?,
            turns: scalar(turns, parameters, Dimension::Scalar)?,
            left_handed: *left_handed,
        }),
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
            bottom,
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
                    bottom,
                    finish,
                    thread: thread.as_deref(),
                },
                shapes,
                definitions,
            )
            .map_err(|error| error.context(&format!("hole input '{input}'")));
        }
        FeatureOperation::Common { left, right } => {
            return session
                .common(shape(shapes, left)?, shape(shapes, right)?)
                .map_err(|error| ModelError::from(error).locate_operands([left, right]));
        }
        FeatureOperation::Unify {
            input,
            linear_tolerance,
            angular_tolerance,
        } => session.unify_same_domain(
            shape(shapes, input)?,
            scalar(linear_tolerance, parameters, Dimension::Length)?,
            scalar(angular_tolerance, parameters, Dimension::Scalar)?,
        ),
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
                definitions,
            );
        }
        FeatureOperation::VariableFillet {
            input,
            edges,
            start_radius,
            end_radius,
            stations,
            spine_direction,
        } => {
            return variable_fillet::execute(
                session,
                shape(shapes, input)?,
                edges,
                variable_fillet::evaluate_law(
                    start_radius,
                    end_radius,
                    stations,
                    spine_direction,
                    parameters,
                )?,
                parameters,
                shapes,
                definitions,
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
                definitions,
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
                let matches = resolve_face_selector(
                    session,
                    input,
                    selector,
                    parameters,
                    shapes,
                    definitions,
                )?;
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
                (
                    scalar(thickness, parameters, Dimension::Length)?,
                    scalar(tolerance, parameters, Dimension::Length)?,
                ),
                parameters,
                shapes,
                definitions,
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
    definitions: &Features<'_>,
) -> Result<Shape<'session>, ModelError> {
    let (selected, sizes) = resolve_edge_selectors(
        session,
        input,
        selectors,
        parameters,
        shapes,
        "fillet",
        definitions,
    )?;
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
    definitions: &Features<'_>,
) -> Result<Shape<'session>, ModelError> {
    let (selected, sizes) = resolve_edge_selectors(
        session,
        input,
        selectors,
        parameters,
        shapes,
        "chamfer",
        definitions,
    )?;
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
    (thickness, tolerance): (f64, f64),
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'session>>,
    definitions: &Features<'_>,
) -> Result<Shape<'session>, ModelError> {
    if selectors.is_empty() {
        return Err(ModelError::new(
            "hollow requires at least one face selector",
        ));
    }
    let mut selected = Vec::new();
    let mut sizes = Vec::with_capacity(selectors.len());
    for selector in selectors {
        match resolve_face_selector(session, input, selector, parameters, shapes, definitions) {
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

fn rib_closure(
    mode: &RibProfileMode,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<ribs::ProfileClosure, ModelError> {
    Ok(match mode {
        RibProfileMode::Closed => ribs::ProfileClosure::Closed,
        RibProfileMode::OpenStrip { offset } => {
            ribs::ProfileClosure::Offset(vector(offset, parameters, Dimension::Length)?)
        }
        RibProfileMode::OpenToNext {
            direction,
            maximum_length,
        } => ribs::ProfileClosure::ToNext {
            direction: vector(direction, parameters, Dimension::Scalar)?,
            maximum_length: scalar(maximum_length, parameters, Dimension::Length)?,
        },
    })
}
