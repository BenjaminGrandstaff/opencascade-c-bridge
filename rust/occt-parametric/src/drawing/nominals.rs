//! Nominal points and normals for measured profile and runout controls, projected onto
//! the regenerated part's own faces (schema 140, ABI 69).
use super::*;
use crate::assembly::{rotate_by, transform_point};

pub trait DrawingDefinitionInspection {
    /// Fills the nominals of each measured profile or runout control that names a
    /// `surface` and was recorded without them: every measured point is
    /// projected onto the nearest of those faces of the attachment output,
    /// giving its nominal point and the outward normal there. Records that
    /// already carry nominals are kept as given.
    ///
    /// Each attachment instance is regenerated once at its own parameters and
    /// its faces are selected there; points move into the instance's local
    /// coordinates and the projections back out through its placement and
    /// enclosing frames. Time: one regeneration per instance + O(points ·
    /// nearby faces) projections; memory O(points).
    fn project_profile_nominals(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        record: &InspectionRecord,
    ) -> Result<InspectionRecord, ModelError>;

    /// [`Self::project_profile_nominals`], then
    /// [`DrawingDefinition::evaluate_inspection`].
    fn evaluate_inspection_on_model(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        record: &InspectionRecord,
    ) -> Result<InspectionReport, ModelError>;
}

impl DrawingDefinitionInspection for DrawingDefinition {
    fn project_profile_nominals(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        record: &InspectionRecord,
    ) -> Result<InspectionRecord, ModelError> {
        self.validate(graph)?;
        let frames: HashMap<&str, &DrawingFeatureControlFrame> = self
            .feature_control_frames
            .iter()
            .map(|f| (f.id.as_str(), f))
            .collect();
        let mut record = record.clone();
        let mut parts: HashMap<String, Part<'_, '_>> = HashMap::new();
        for measured in &mut record.controls {
            let Some(frame) = frames.get(measured.id.as_str()) else {
                continue;
            };
            let Some(surface) = frame.surface.as_ref().or(frame
                .attachment
                .faces
                .as_ref()
                .filter(|_| frame.characteristic.nominal_surface()))
            else {
                continue;
            };
            if !measured.nominals.is_empty() {
                continue;
            }
            let instance = &frame.attachment.output.instance;
            if !parts.contains_key(instance) {
                parts.insert(instance.clone(), Part::new(graph, session, instance)?);
            }
            let part = &parts[instance];
            let faces = part.instance.select_faces(
                session,
                &part.generated,
                &frame.attachment.output.output,
                surface,
            )?;
            if faces.is_empty() {
                return Err(ModelError::new(format!(
                    "profile surface of '{}' selects no faces",
                    frame.id
                )));
            }
            let local = measured
                .points_mm
                .iter()
                .map(|p| part.local(Vec3::new(p[0], p[1], p[2])))
                .collect::<Vec<_>>();
            let faces: Vec<&Shape<'_>> = faces.iter().collect();
            measured.nominals = session
                .project_points_to_faces(&faces, &local)?
                .into_iter()
                .map(|projection| {
                    let point = part.model(projection.point);
                    let normal = part.turn(projection.normal);
                    MeasuredNominal {
                        point_mm: [point.x, point.y, point.z],
                        normal: [normal.x, normal.y, normal.z],
                    }
                })
                .collect();
        }
        Ok(record)
    }

    fn evaluate_inspection_on_model(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        record: &InspectionRecord,
    ) -> Result<InspectionReport, ModelError> {
        let projected = self.project_profile_nominals(graph, session, record)?;
        self.evaluate_inspection(graph, &projected)
    }
}

/// A locally regenerated instance and the placements, innermost first, that
/// carry its local coordinates into the model.
struct Part<'definition, 'session> {
    instance: PartInstance<'definition>,
    generated: GeneratedResult<'session>,
    placements: Vec<NormalizedPlacement>,
}

impl<'definition, 'session> Part<'definition, 'session> {
    fn new(
        graph: &InstanceGraph<'definition>,
        session: &'session Session,
        id: &str,
    ) -> Result<Self, ModelError> {
        let resolved = graph.resolve_with_placement(id)?;
        let placements = std::iter::once(resolved.placement)
            .chain(resolved.frames)
            .map(|p| p.normalized())
            .collect::<Result<Vec<_>, _>>()?;
        let generated = resolved.instance.regenerate(session)?;
        Ok(Self {
            instance: resolved.instance,
            generated,
            placements,
        })
    }

    fn model(&self, point: Vec3) -> Vec3 {
        self.placements.iter().fold(point, transform_point)
    }

    fn turn(&self, direction: Vec3) -> Vec3 {
        self.placements.iter().fold(direction, rotate_by)
    }

    /// Inverse of [`Self::model`].
    fn local(&self, point: Vec3) -> Vec3 {
        self.placements.iter().rev().fold(point, |p, placement| {
            let p = subtract(p, placement.translation);
            match placement.rotation {
                Some((origin, axis, angle)) => {
                    let inverse = NormalizedPlacement {
                        translation: Vec3::new(0.0, 0.0, 0.0),
                        rotation: Some((origin, axis, -angle)),
                    };
                    add(origin, rotate_by(subtract(p, origin), &inverse))
                }
                None => p,
            }
        })
    }
}
