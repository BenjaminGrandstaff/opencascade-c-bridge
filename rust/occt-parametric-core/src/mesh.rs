//! Tagged tessellation and rendering/analysis hand-off from explicit outputs.
use crate::*;
use occt_types::{MeshOptions, MeshTriangle};
pub mod manufacturing;
pub use manufacturing::*;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
    pub fn validate(&self) -> Result<(), ModelError> {
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn options(self) -> Result<MeshOptions, ModelError> {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MeshFaceTag {
    pub id: String,
    pub faces: Vec<FaceSelector>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

    pub fn validate_cached<'definition>(
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
}

pub struct ExportContext<'definition> {
    pub resolutions: graph::ResolutionCache<'definition>,
    pub suppressed: HashSet<String>,
    pub features: HashMap<String, HashSet<String>>,
    pub material_ids: HashMap<String, Option<String>>,
}
impl<'definition> ExportContext<'definition> {
    pub fn new(graph: &InstanceGraph<'definition>) -> Self {
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

pub fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.len() <= 128
        && !name
            .chars()
            .any(|character| character.is_control() || matches!(character, '"' | '\\'))
}
pub fn xyz(point: Vec3) -> [f64; 3] {
    [point.x, point.y, point.z]
}

pub mod export;
