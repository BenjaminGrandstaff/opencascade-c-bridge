//! Tagged tessellation and rendering/analysis hand-off from explicit outputs.
use crate::*;
use occt_bridge::{MeshOptions, MeshTriangle};
use std::path::Path;

mod export;
mod manufacturing;
pub use manufacturing::*;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaterialAppearance {
    /// Linear RGBA, each component in [0, 1].
    pub base_color: [f64; 4],
    pub metallic: f64,
    pub roughness: f64,
    #[serde(default)]
    pub double_sided: bool,
}
impl Default for MaterialAppearance {
    fn default() -> Self {
        Self {
            base_color: [0.8, 0.8, 0.8, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            double_sided: false,
        }
    }
}
impl MaterialAppearance {
    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        if self
            .base_color
            .into_iter()
            .chain([self.metallic, self.roughness])
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(ModelError::new(
                "appearance color, metallic, and roughness must be finite in [0,1]",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshSettings {
    pub linear_deflection: Quantity,
    pub angular_deflection_radians: f64,
    pub maximum_triangles: usize,
}
impl Default for MeshSettings {
    fn default() -> Self {
        Self {
            linear_deflection: Quantity::length(0.1, LengthUnit::Millimeter),
            angular_deflection_radians: 0.3,
            maximum_triangles: 1_000_000,
        }
    }
}
impl MeshSettings {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn options(self) -> Result<MeshOptions, ModelError> {
        if self.linear_deflection.dimension != Dimension::Length {
            return Err(ModelError::new("mesh deflection must be a length"));
        }
        let linear_deflection = self.linear_deflection.normalized()?;
        if !linear_deflection.is_finite()
            || linear_deflection <= 0.0
            || !self.angular_deflection_radians.is_finite()
            || !(0.01..=std::f64::consts::PI).contains(&self.angular_deflection_radians)
            || !(1..=1_000_000).contains(&self.maximum_triangles)
        {
            return Err(ModelError::new(
                "invalid mesh deflection or triangle budget",
            ));
        }
        Ok(MeshOptions {
            linear_deflection,
            angular_deflection_radians: self.angular_deflection_radians,
            maximum_triangles: self.maximum_triangles,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshFaceTag {
    pub id: String,
    pub faces: Vec<FaceSelector>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshExportDefinition {
    pub id: String,
    pub output: InstanceOutputRef,
    #[serde(default)]
    pub settings: MeshSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub face_tags: Vec<MeshFaceTag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturing: Option<ManufacturingSettings>,
}

/// Exact geometric descriptors let an external volume mesher match imported
/// BREP faces without assuming that its topology numbers preserve OCCT order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshFaceDescriptor {
    pub index: usize,
    pub physical_tag: u32,
    pub area_mm2: f64,
    pub center_mm: [f64; 3],
    pub bounds_mm: [[f64; 3]; 2],
}
#[derive(Clone, Debug, PartialEq)]
pub struct TaggedSurfaceMesh {
    pub id: String,
    pub triangles: Vec<MeshTriangle>,
    /// Physical tags: zero is unassigned; positive tags index `names` minus one.
    pub face_tags: Vec<u32>,
    pub names: Vec<String>,
    pub faces: Vec<MeshFaceDescriptor>,
}

impl MeshExportDefinition {
    pub fn validate(&self, graph: &InstanceGraph<'_>) -> Result<(), ModelError> {
        self.validate_cached(graph, &mut ExportContext::new(graph))
    }

    pub(crate) fn validate_cached<'definition>(
        &self,
        graph: &InstanceGraph<'definition>,
        context: &mut ExportContext<'definition>,
    ) -> Result<(), ModelError> {
        if !valid_name(&self.id)
            || self.id == "__unassigned_boundary"
            || self.face_tags.len() > 1024
        {
            return Err(ModelError::new(
                "mesh id must be a safe nonempty name; at most 1024 face tags",
            ));
        }
        self.settings.options()?;
        if let Some(settings) = self.manufacturing {
            manufacturing::validate(settings)?;
        }
        if context.suppressed.contains(&self.output.instance) {
            return Err(ModelError::new("mesh output instance is suppressed"));
        }
        let instance = graph.resolve_cached(&self.output.instance, &mut context.resolutions)?;
        if !context
            .features
            .get(&instance.definition.id)
            .is_some_and(|features| features.contains(&self.output.output))
        {
            return Err(ModelError::new("mesh output does not exist"));
        }
        let parameters = resolve_parameters(instance.definition, &instance.overrides)?;
        let mut ids = HashSet::new();
        for tag in &self.face_tags {
            if !valid_name(&tag.id)
                || tag.id == "__unassigned_boundary"
                || tag.id == self.id
                || !ids.insert(&tag.id)
                || tag.faces.is_empty()
            {
                return Err(ModelError::new(
                    "face tags need safe unique names and nonempty selectors",
                ));
            }
            let mut names = HashSet::new();
            for selector in &tag.faces {
                regeneration::collect_face_selector_parameters(selector, &mut names);
            }
            if names.iter().any(|name| !parameters.contains_key(*name)) {
                return Err(ModelError::new(
                    "mesh face tag references an unknown parameter",
                ));
            }
        }
        Ok(())
    }

    pub fn generate<'definition>(
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
        self.tagged_result(graph, session, result, &mut context)
    }

    fn tagged_result<'definition>(
        &self,
        graph: &InstanceGraph<'definition>,
        session: &Session,
        result: &GeneratedResult<'_>,
        context: &mut ExportContext<'definition>,
    ) -> Result<TaggedSurfaceMesh, ModelError> {
        let shape = result
            .shape(&self.output.output)
            .ok_or_else(|| ModelError::new("mesh output was not generated"))?;
        let instance = graph.resolve_cached(&self.output.instance, &mut context.resolutions)?;
        let parameters = resolve_parameters(instance.definition, &instance.overrides)?;
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

    /// Create a new directory containing body.brep, surface.msh, and volume.json
    /// for tools/mesh/volume.py. An existing directory is never overwritten.
    /// A single valid solid is required for the volume hand-off.
    pub fn write_fea_bundle(
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
        let mesh = self.tagged_result(graph, session, result, &mut context)?;
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

pub(crate) struct ExportContext<'definition> {
    resolutions: graph::ResolutionCache<'definition>,
    suppressed: HashSet<String>,
    features: HashMap<String, HashSet<String>>,
    material_ids: HashMap<String, Option<String>>,
}
impl<'definition> ExportContext<'definition> {
    pub(crate) fn new(graph: &InstanceGraph<'definition>) -> Self {
        let mut suppressed: HashSet<_> = graph
            .patterns
            .iter()
            .flat_map(|pattern| &pattern.members)
            .filter(|member| member.suppressed)
            .map(|member| member.id.clone())
            .collect();
        if let Some(configuration) = graph.assembly.active_configuration.as_ref().and_then(|id| {
            graph
                .assembly
                .configurations
                .iter()
                .find(|configuration| &configuration.id == id)
        }) {
            suppressed.extend(configuration.suppressed.iter().cloned());
        }
        let features = std::iter::once(graph.definition)
            .chain(graph.additional_definitions.values().copied())
            .map(|family| {
                (
                    family.id.clone(),
                    family
                        .features
                        .iter()
                        .map(|feature| feature.id.clone())
                        .collect(),
                )
            })
            .collect();
        Self {
            resolutions: HashMap::new(),
            suppressed,
            features,
            material_ids: HashMap::new(),
        }
    }
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.len() <= 128
        && !name
            .chars()
            .any(|character| character.is_control() || matches!(character, '"' | '\\'))
}
fn xyz(point: Vec3) -> [f64; 3] {
    [point.x, point.y, point.z]
}
