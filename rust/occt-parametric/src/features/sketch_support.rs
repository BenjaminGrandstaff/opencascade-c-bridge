//! Semantic face-attached sketch planes. Query work follows the existing face
//! selector and native topology/area queries; no persistent topology indices.
use super::*;
use crate::assembly::{add, scale, unit};

pub(super) fn resolve(
    session: &Session,
    sketch: &SketchDefinition,
    parameters: &HashMap<String, ParameterValue>,
    shapes: &HashMap<String, Shape<'_>>,
    definitions: &Features<'_>,
) -> Result<Option<ResolvedDatum>, ModelError> {
    let Some(support) = &sketch.face_support else {
        return Ok(None);
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

impl PartInstance<'_> {
    /// Resolve attached sketch planes against an already generated local result.
    /// Builds parameter/feature indexes once. Cost is O(features + parameter
    /// resolution + sum of support-query costs), with one plane/error per support.
    ///
    /// Returned planes are values; native query handles are released immediately.
    pub fn sketch_support_planes(
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
}
