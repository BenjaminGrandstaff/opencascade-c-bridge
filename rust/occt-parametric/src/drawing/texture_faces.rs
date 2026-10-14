//! Surface texture requirements on the persistent faces they govern.
use super::nominals::Part;
use super::*;

/// The faces of one output a surface texture requirement governs.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SurfaceTextureFaces {
    pub texture: String,
    pub output: InstanceOutputRef,
    /// Indices of the governed faces among the output's faces, ascending.
    pub faces: Vec<usize>,
    /// Total area of those faces.
    pub area_mm2: f64,
    /// The point on those faces nearest the attachment's anchor, in model
    /// coordinates, where the drawing's leader lands.
    pub landing_mm: [f64; 3],
}

pub trait DrawingSurfaceTextureFaces {
    /// The faces each surface texture whose attachment names `faces`
    /// governs, in drawing order. Faces are selected on one local
    /// regeneration per textured instance, where its selectors are defined;
    /// placement keeps face order, so the indices hold on the placed output.
    /// Selectors that select nothing fail, as does a face governed by two
    /// textures whose requirements differ (all but ID, attachment and
    /// all-around); repeating one requirement in several views is allowed.
    /// Time: one regeneration per textured instance, one selection, face
    /// lookup and batched anchor projection per distinct selector on an
    /// output, and O(textures × selected faces) conflict checks.
    fn surface_texture_faces(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
    ) -> Result<Vec<SurfaceTextureFaces>, ModelError>;
}

/// Whether two textures state the same requirement.
fn same_requirement(a: &DrawingSurfaceTexture, b: &DrawingSurfaceTexture) -> bool {
    a.unit == b.unit
        && a.roughness == b.roughness
        && a.cutoff_mm == b.cutoff_mm
        && a.waviness == b.waviness
        && a.lay == b.lay
        && a.material_removal == b.material_removal
        && a.method == b.method
        && a.acceptance == b.acceptance
}

/// Faces one selector picks on one output, shared by every texture naming it.
struct Selection<'session> {
    shapes: Vec<Shape<'session>>,
    faces: Vec<usize>,
    area_mm2: f64,
    /// Textures on these faces and their anchors, in local coordinates.
    textures: Vec<(usize, Vec3)>,
}

impl DrawingSurfaceTextureFaces for DrawingDefinition {
    fn surface_texture_faces(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
    ) -> Result<Vec<SurfaceTextureFaces>, ModelError> {
        self.validate(graph)?;
        let mut parts: HashMap<String, Part<'_, '_>> = HashMap::new();
        let mut governed: HashMap<(&InstanceOutputRef, usize), &DrawingSurfaceTexture> =
            HashMap::new();
        // By output and selector, in first-use order.
        let mut keys: HashMap<(&InstanceOutputRef, String), usize> = HashMap::new();
        let mut selections: Vec<(&InstanceOutputRef, Selection<'_>)> = Vec::new();
        for (index, texture) in self.surface_textures.iter().enumerate() {
            let attachment = &texture.attachment;
            let Some(selector) = &attachment.faces else {
                continue;
            };
            let output = &attachment.output;
            if !parts.contains_key(&output.instance) {
                parts.insert(
                    output.instance.clone(),
                    Part::new(graph, session, &output.instance)?,
                );
            }
            let part = &parts[&output.instance];
            let key = (
                output,
                serde_json::to_string(selector).map_err(|e| ModelError::new(e.to_string()))?,
            );
            let slot = match keys.get(&key) {
                Some(&slot) => slot,
                None => {
                    let shapes = part.instance.select_faces(
                        session,
                        &part.generated,
                        &output.output,
                        selector,
                    )?;
                    if shapes.is_empty() {
                        return Err(ModelError::new(format!(
                            "faces of surface texture '{}' select no faces",
                            texture.id
                        )));
                    }
                    let shape = part.generated.shape(&output.output).ok_or_else(|| {
                        ModelError::new(format!("unknown generated output '{}'", output.output))
                    })?;
                    let references: Vec<&Shape<'_>> = shapes.iter().collect();
                    let mut faces = session
                        .subshape_lookup(shape, ShapeType::Face, &references)?
                        .into_iter()
                        .map(|index| {
                            index.ok_or_else(|| {
                                ModelError::new(format!(
                                    "faces of surface texture '{}' are not faces of its output",
                                    texture.id
                                ))
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    faces.sort_unstable();
                    faces.dedup();
                    let area_mm2 = references
                        .iter()
                        .map(|face| session.surface_area(face))
                        .sum::<Result<f64, _>>()?;
                    selections.push((
                        output,
                        Selection {
                            shapes,
                            faces,
                            area_mm2,
                            textures: Vec::new(),
                        },
                    ));
                    keys.insert(key, selections.len() - 1);
                    selections.len() - 1
                }
            };
            for &face in &selections[slot].1.faces {
                if let Some(other) = governed.insert((output, face), texture)
                    && !same_requirement(other, texture)
                {
                    return Err(ModelError::new(format!(
                        "face {face} of '{}:{}' is governed by surface textures '{}' and '{}' with different requirements",
                        output.instance, output.output, other.id, texture.id
                    )));
                }
            }
            let anchor = match graph.datum(&attachment.anchor.instance, &attachment.anchor.datum)? {
                ResolvedDatum::Point { origin }
                | ResolvedDatum::Axis { origin, .. }
                | ResolvedDatum::Plane { origin, .. } => origin,
            };
            selections[slot]
                .1
                .textures
                .push((index, part.local(anchor)));
        }
        // One projection per selection, of all its textures' anchors.
        let mut result: Vec<(usize, SurfaceTextureFaces)> = Vec::new();
        for (output, selection) in &selections {
            let part = &parts[&output.instance];
            let references: Vec<&Shape<'_>> = selection.shapes.iter().collect();
            let anchors: Vec<Vec3> = selection.textures.iter().map(|(_, a)| *a).collect();
            let projections = session.project_points_to_faces(&references, &anchors)?;
            if projections.len() != anchors.len() {
                return Err(ModelError::new("projection returned the wrong point count"));
            }
            for ((index, _), projection) in selection.textures.iter().zip(projections) {
                let landing = part.model(projection.point);
                result.push((
                    *index,
                    SurfaceTextureFaces {
                        texture: self.surface_textures[*index].id.clone(),
                        output: (*output).clone(),
                        faces: selection.faces.clone(),
                        area_mm2: selection.area_mm2,
                        landing_mm: [landing.x, landing.y, landing.z],
                    },
                ));
            }
        }
        result.sort_unstable_by_key(|(index, _)| *index);
        Ok(result.into_iter().map(|(_, faces)| faces).collect())
    }
}
