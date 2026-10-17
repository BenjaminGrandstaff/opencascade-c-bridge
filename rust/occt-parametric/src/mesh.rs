//! Tagged tessellation and rendering/analysis hand-off from explicit outputs.
use crate::*;
use occt_bridge::MeshTriangle;
#[allow(unused_imports)]
pub use occt_parametric_core::mesh::*;
use std::path::Path;

mod export;
pub use export::InstanceGraphExport;
pub use export::TaggedSurfaceMeshExport;
pub use export::gltf_to_glb;
pub use export::{InstanceGraphParallelExport, ParallelGltf};
pub trait MeshExportDefinitionMeshExport {
    fn generate<'definition>(
        &self,
        graph: &InstanceGraph<'definition>,
        session: &Session,
    ) -> Result<TaggedSurfaceMesh, ModelError>;

    /// Create a new directory containing body.brep, surface.msh, and volume.json
    /// for tools/mesh/volume.py. An existing directory is never overwritten.
    /// A single valid solid is required for the volume hand-off.
    fn write_fea_bundle(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        directory: &Path,
    ) -> Result<(), ModelError>;
}

impl MeshExportDefinitionMeshExport for MeshExportDefinition {
    fn generate<'definition>(
        &self,
        graph: &InstanceGraph<'definition>,
        session: &Session,
    ) -> Result<TaggedSurfaceMesh, ModelError> {
        let mut context = ExportContext::new(graph);
        self.validate_cached(graph, &mut context)?;
        let generated =
            graph.regenerate_instances_current(session, &[self.output.instance.as_str()])?;
        let result = generated
            .result(&self.output.instance)
            .ok_or_else(|| ModelError::new("mesh instance was not generated"))?;
        self.tagged_result(graph, session, result, &mut context, true)
    }

    fn write_fea_bundle(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        directory: &Path,
    ) -> Result<(), ModelError> {
        let mut context = ExportContext::new(graph);
        self.validate_cached(graph, &mut context)?;
        let generated =
            graph.regenerate_instances_current(session, &[self.output.instance.as_str()])?;
        let result = generated
            .result(&self.output.instance)
            .ok_or_else(|| ModelError::new("mesh instance was not generated"))?;
        let shape = result
            .shape(&self.output.output)
            .ok_or_else(|| ModelError::new("mesh output was not generated"))?;
        if session.subshape_count(shape, ShapeType::Solid)? != 1 || !session.is_valid(shape)? {
            return Err(ModelError::new(
                "volume mesh hand-off requires exactly one valid solid",
            ));
        }
        let mesh = self.tagged_result(graph, session, result, &mut context, true)?;
        let manifest = serde_json::json!({ "version": 1, "units": "mm", "volume_name": self.id,
            "face_tags": mesh.names, "faces": mesh.faces });
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|error| ModelError::new(error.to_string()))?;
        let surface = mesh.to_msh()?;
        std::fs::create_dir(directory)
            .map_err(|error| ModelError::new(format!("create FEA bundle: {error}")))?;
        let outcome = (|| {
            session.save_brep(shape, directory.join("body.brep"))?;
            std::fs::write(directory.join("surface.msh"), surface)
                .map_err(|error| ModelError::new(error.to_string()))?;
            std::fs::write(directory.join("volume.json"), json)
                .map_err(|error| ModelError::new(error.to_string()))
        })();
        if outcome.is_err() {
            let _ = std::fs::remove_dir_all(directory);
        }
        outcome
    }
}

pub(crate) trait MeshExportDefinitionMeshExportInternal {
    fn tagged_result<'definition>(
        &self,
        graph: &InstanceGraph<'definition>,
        session: &Session,
        result: &GeneratedResult<'_>,
        context: &mut ExportContext<'definition>,
        // Exact per-face descriptors (area, center, bounds) for volume
        // meshers; renderers such as glTF skip their per-face cost.
        descriptors: bool,
    ) -> Result<TaggedSurfaceMesh, ModelError>;
}

impl MeshExportDefinitionMeshExportInternal for MeshExportDefinition {
    fn tagged_result<'definition>(
        &self,
        graph: &InstanceGraph<'definition>,
        session: &Session,
        result: &GeneratedResult<'_>,
        context: &mut ExportContext<'definition>,
        // Exact per-face descriptors (area, center, bounds) for volume
        // meshers; renderers such as glTF skip their per-face cost.
        descriptors: bool,
    ) -> Result<TaggedSurfaceMesh, ModelError> {
        let shape = result
            .shape(&self.output.output)
            .ok_or_else(|| ModelError::new("mesh output was not generated"))?;
        let instance = graph.resolve_cached(&self.output.instance, &mut context.resolutions)?;
        let parameters = resolve_parameters(instance.definition, &instance.overrides)?;
        let definitions = selection::Features::new(instance.definition);
        let faces = session.subshapes(shape, ShapeType::Face)?;
        let mut face_tags = vec![0; faces.len()];
        let mut selected = Vec::new();
        let mut physical_tags = Vec::new();
        for (index, tag) in self.face_tags.iter().enumerate() {
            for selector in &tag.faces {
                let faces = selection::resolve_face_selector(
                    session,
                    shape,
                    selector,
                    &parameters,
                    &result.shapes,
                    &definitions,
                )?;
                physical_tags.extend(std::iter::repeat_n((index + 1) as u32, faces.len()));
                selected.extend(faces);
            }
        }
        let references: Vec<_> = selected.iter().collect();
        for (face, physical) in session
            .subshape_indices(shape, ShapeType::Face, &references)?
            .into_iter()
            .zip(physical_tags)
        {
            if face_tags[face] != 0 && face_tags[face] != physical {
                return Err(ModelError::new("different physical face tags overlap"));
            }
            face_tags[face] = physical;
        }
        let descriptors = faces
            .iter()
            .take(if descriptors { faces.len() } else { 0 })
            .enumerate()
            .map(|(index, face)| {
                let center = session.center_of_mass(face)?;
                let bounds = session.exact_bounds(face)?;
                Ok(MeshFaceDescriptor {
                    index,
                    physical_tag: face_tags[index],
                    area_mm2: session.surface_area(face)?,
                    center_mm: xyz(center),
                    bounds_mm: [xyz(bounds.min), xyz(bounds.max)],
                })
            })
            .collect::<Result<_, ModelError>>()?;
        let triangles = session.surface_mesh(shape, self.settings.options()?)?;
        Ok(TaggedSurfaceMesh {
            id: self.id.clone(),
            triangles,
            face_tags,
            names: self.face_tags.iter().map(|tag| tag.id.clone()).collect(),
            faces: descriptors,
        })
    }
}
