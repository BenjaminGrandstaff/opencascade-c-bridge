//! Semantic face-attached sketch planes. Query work follows the existing face
//! selector and native topology/area queries; no persistent topology indices.
use super::*;
use crate::assembly::{add, scale, unit};

/// Planar faces of the outputs generated so far, for datum planes built on
/// faces. Each query is one face-selector resolution.
pub(crate) struct GeneratedFaces<'a, 's> {
    pub(crate) session: &'a Session,
    pub(crate) parameters: &'a HashMap<String, ParameterValue>,
    pub(crate) shapes: &'a HashMap<String, Shape<'s>>,
    pub(crate) definitions: &'a Features<'a>,
}

impl PlaneFaces for GeneratedFaces<'_, '_> {
    fn planar_face(&self, input: &str, face: &FaceSelector) -> Result<(Vec3, Vec3), ModelError> {
        let source = shape(self.shapes, input)?;
        let faces = resolve_face_selector(
            self.session,
            source,
            face,
            self.parameters,
            self.shapes,
            self.definitions,
        )?;
        if faces.len() != 1 {
            return Err(ModelError::new(format!(
                "datum plane face on '{input}' must select exactly one face (selected {})",
                faces.len()
            )));
        }
        if !self.session.face_is_planar(&faces[0])? {
            return Err(ModelError::new("datum plane face must be planar"));
        }
        Ok((
            self.session.center_of_mass(&faces[0])?,
            unit(self.session.face_normal(&faces[0])?)?,
        ))
    }
}

/// The plane a sketch is drawn on: its face support, or its datum plane
/// resolved through any base datums and faces; `None` for a sketch placed
/// by its own origin and axes.
pub(crate) fn resolve(
    session: &Session,
    sketch: &SketchDefinition,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'_>>,
    definitions: &Features<'_>,
) -> Result<Option<ResolvedDatum>, ModelError> {
    let Some(support) = &sketch.face_support else {
        let Some(id) = &sketch.datum_plane else {
            return Ok(None);
        };
        let datum = definitions
            .datums
            .get(id.as_str())
            .ok_or_else(|| ModelError::new(format!("unknown sketch plane datum '{id}'")))?;
        if !datum.kind.is_plane() {
            return Err(ModelError::new(format!(
                "sketch datum '{id}' must be a plane"
            )));
        }
        let faces = GeneratedFaces {
            session,
            parameters,
            shapes,
            definitions,
        };
        return cached_plane(definitions, id, parameters, &faces, 0).map(Some);
    };
    if sketch.datum_plane.is_some() {
        return Err(ModelError::new(
            "sketch face support and datum plane are mutually exclusive",
        ));
    }
    let source = shape(shapes, &support.input)?;
    let faces = resolve_face_selector(
        session,
        source,
        &support.face,
        parameters,
        shapes,
        definitions,
    )?;
    if faces.len() != 1 {
        return Err(ModelError::new(format!(
            "sketch support '{}' must select exactly one face (selected {})",
            support.input,
            faces.len()
        )));
    }
    let face = &faces[0];
    if !session.face_is_planar(face)? {
        return Err(ModelError::new("sketch support face must be planar"));
    }
    let normal = unit(session.face_normal(face)?)?;
    let centre = session.center_of_mass(face)?;
    let offset = scalar(&support.offset, parameters, Dimension::Length)?;
    let origin = add(centre, scale(normal, offset));
    if ![origin.x, origin.y, origin.z].iter().all(|v| v.is_finite()) {
        return Err(ModelError::new(
            "sketch support origin exceeds finite coordinates",
        ));
    }
    Ok(Some(ResolvedDatum::Plane { origin, normal }))
}
/// Resolves datum `id` through its bases, reusing planes already resolved
/// in this context: n sketches on one chain of depth d cost O(n + d) plane
/// constructions instead of O(n × d).
fn cached_plane(
    definitions: &Features<'_>,
    id: &str,
    parameters: &HashMap<String, ParameterValue>,
    faces: &GeneratedFaces<'_, '_>,
    depth: usize,
) -> Result<ResolvedDatum, ModelError> {
    if let Some(plane) = definitions.planes.borrow().get(id) {
        return Ok(*plane);
    }
    if depth > definitions.datums.len() {
        return Err(ModelError::new(format!(
            "datum plane '{id}' is built on itself through its bases"
        )));
    }
    let datum = definitions
        .datums
        .get(id)
        .ok_or_else(|| ModelError::new(format!("unknown datum '{id}'")))?;
    let plane = datum.kind.resolve(
        parameters,
        &|base| cached_plane(definitions, base, parameters, faces, depth + 1),
        faces,
    )?;
    definitions.planes.borrow_mut().insert(id.to_owned(), plane);
    Ok(plane)
}

/// Each plane datum's id with its resolved plane or its own error.
pub type DatumPlanes = Vec<(String, Result<ResolvedDatum, ModelError>)>;

pub trait PartInstanceSupport {
    /// Resolve attached sketch planes against an already generated local result.
    /// Builds parameter/feature indexes once. Cost is O(features + parameter
    /// resolution + sum of support-query costs), with one plane/error per support.
    ///
    /// Returned planes are values; native query handles are released immediately.
    fn sketch_support_planes(
        &self,
        session: &Session,
        generated: &GeneratedResult<'_>,
    ) -> Result<SketchSupportPlanes, ModelError>;

    /// Every plane datum resolved against an already generated local
    /// result, by id, each with its own error. O(datums × chain length ×
    /// face-query cost).
    fn datum_planes(
        &self,
        session: &Session,
        generated: &GeneratedResult<'_>,
    ) -> Result<DatumPlanes, ModelError>;
}

impl PartInstanceSupport for PartInstance<'_> {
    fn sketch_support_planes(
        &self,
        session: &Session,
        generated: &GeneratedResult<'_>,
    ) -> Result<SketchSupportPlanes, ModelError> {
        let parameters = self.resolved_parameters()?;
        let definitions = Features::new(self.definition);
        let mut planes = HashMap::new();
        for feature in &self.definition.features {
            let sketch = match &feature.operation {
                FeatureOperation::SketchFace { sketch }
                | FeatureOperation::SketchWire { sketch }
                | FeatureOperation::SketchOpenWire { sketch }
                    if sketch.face_support.is_some() =>
                {
                    sketch
                }
                _ => continue,
            };
            planes.insert(
                feature.id.clone(),
                resolve(
                    session,
                    sketch,
                    &parameters,
                    &generated.shapes,
                    &definitions,
                )
                .and_then(|p| p.ok_or_else(|| ModelError::new("missing sketch support"))),
            );
        }
        Ok(planes)
    }

    fn datum_planes(
        &self,
        session: &Session,
        generated: &GeneratedResult<'_>,
    ) -> Result<DatumPlanes, ModelError> {
        let parameters = self.resolved_parameters()?;
        let definitions = Features::new(self.definition);
        let faces = GeneratedFaces {
            session,
            parameters: &parameters,
            shapes: &generated.shapes,
            definitions: &definitions,
        };
        Ok(self
            .definition
            .datums
            .iter()
            .filter(|datum| datum.kind.is_plane())
            .map(|datum| {
                (
                    datum.id.clone(),
                    cached_plane(&definitions, &datum.id, &parameters, &faces, 0),
                )
            })
            .collect())
    }
}
