//! Verification of family requirements against regenerated geometry.

use super::*;

/// Evaluates every requirement; required failures reject the generation.
pub(crate) fn verify_requirements(
    session: &Session,
    definition: &FamilyDefinition,
    shapes: &HashMap<String, Shape<'_>>,
) -> Result<Vec<VerificationResult>, ModelError> {
    let mut verification = Vec::new();
    let mut required_failures = Vec::new();
    for requirement in &definition.requirements {
        let result = verify_requirement(session, requirement, shapes).map_err(|error| {
            ModelError::new(format!("requirement '{}': {error}", requirement.id))
        })?;
        if requirement.priority == RequirementPriority::Required
            && result.status == VerificationStatus::Failed
        {
            required_failures.push(requirement.id.clone());
        }
        verification.push(result);
    }
    if !required_failures.is_empty() {
        return Err(ModelError::new(format!(
            "required verification failed: {}",
            required_failures.join(", ")
        )));
    }
    Ok(verification)
}

pub(crate) fn verify_requirement(
    session: &Session,
    requirement: &Requirement,
    shapes: &HashMap<String, Shape<'_>>,
) -> Result<VerificationResult, ModelError> {
    let id = requirement.id.as_str();
    Ok(match &requirement.rule {
        VerificationRule::ShapeValid { output } => {
            let passed = session.is_valid(shape(shapes, output)?)?;
            let message = if passed {
                "shape is valid"
            } else {
                "shape is invalid"
            };
            VerificationResult::exact(id, passed, message.into())
        }
        VerificationRule::VolumeRange {
            output,
            minimum,
            maximum,
        } => {
            let volume = session.volume(shape(shapes, output)?)?;
            let minimum = minimum.cubic_millimeters()?;
            let maximum = maximum.cubic_millimeters()?;
            let passed = volume >= minimum && volume <= maximum;
            VerificationResult::exact(
                id,
                passed,
                format!("volume {volume} mm^3; expected {minimum}..={maximum} mm^3"),
            )
            .measured(Measurement {
                value: volume,
                unit: MeasurementUnit::CubicMillimeter,
                minimum: Some(minimum),
                maximum: Some(maximum),
            })
        }
        VerificationRule::Connectivity {
            output,
            solids,
            allow_voids,
        } => {
            if *solids == 0 {
                return Err(ModelError::new("connectivity requires at least one solid"));
            }
            let found = connectivity(session, shape(shapes, output)?)?;
            let loose = found.loose_faces + found.loose_edges + found.loose_vertices;
            let voids_ok = *allow_voids || found.maximum_shells_per_solid <= 1;
            let passed = found.solids == *solids as usize && loose == 0 && voids_ok;
            VerificationResult::exact(
                id,
                passed,
                format!(
                    "{} solid(s), expected {solids}; up to {} shell(s) per solid{}; \
                     {} loose face(s), {} loose edge(s), {} loose vertex(es)",
                    found.solids,
                    found.maximum_shells_per_solid,
                    if *allow_voids { " (voids allowed)" } else { "" },
                    found.loose_faces,
                    found.loose_edges,
                    found.loose_vertices,
                ),
            )
            .measured(Measurement {
                value: found.solids as f64,
                unit: MeasurementUnit::Count,
                minimum: Some(f64::from(*solids)),
                maximum: Some(f64::from(*solids)),
            })
        }
        VerificationRule::MinimumRadius {
            output,
            minimum,
            side,
            sharp_edges,
            samples_per_direction,
        } => minimum_radius(
            session,
            id,
            shape(shapes, output)?,
            *minimum,
            *side,
            *sharp_edges,
            *samples_per_direction,
        )?,
        VerificationRule::FitsWithin { output, envelope } => {
            fits_within(session, id, shape(shapes, output)?, *envelope)?
        }
        VerificationRule::MinimumWall {
            output,
            minimum,
            mesh,
            maximum_samples,
        } => screen(
            session,
            id,
            shape(shapes, output)?,
            *mesh,
            Screen::Wall {
                minimum: *minimum,
                maximum_samples: *maximum_samples,
            },
        )?,
        VerificationRule::DraftAngle {
            output,
            pull_direction,
            minimum_radians,
            mesh,
        } => screen(
            session,
            id,
            shape(shapes, output)?,
            *mesh,
            Screen::Draft {
                pull_direction: *pull_direction,
                minimum_radians: *minimum_radians,
            },
        )?,
        VerificationRule::Undercut {
            output,
            pull_direction,
            parting_origin,
            tolerance_radians,
            mesh,
        } => undercut(
            session,
            id,
            shape(shapes, output)?,
            *mesh,
            *pull_direction,
            *parting_origin,
            *tolerance_radians,
        )?,
        VerificationRule::Overhang {
            output,
            build_direction,
            maximum_radians,
            mesh,
        } => screen(
            session,
            id,
            shape(shapes, output)?,
            *mesh,
            Screen::Overhang {
                build_direction: *build_direction,
                maximum_radians: *maximum_radians,
            },
        )?,
    })
}
