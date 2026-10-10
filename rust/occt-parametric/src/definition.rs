//! Family definitions: parameters, expressions, selectors, feature
//! operations, requirements, and constraints.

use super::*;

mod features;
mod parameters;
mod requirements;
mod selectors;

pub use features::*;
pub use parameters::*;
pub use requirements::*;
pub use selectors::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FamilyDefinition {
    pub id: String,
    pub version: u32,
    pub parameters: Vec<ParameterDefinition>,
    #[serde(default)]
    pub derived_parameters: Vec<DerivedParameterDefinition>,
    #[serde(default)]
    pub derived_vector_parameters: Vec<DerivedVectorParameterDefinition>,
    #[serde(default)]
    pub constraints: Vec<ParameterConstraint>,
    pub features: Vec<FeatureDefinition>,
    pub requirements: Vec<Requirement>,
    /// Named points, axes, and planes in family coordinates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datums: Vec<DatumDefinition>,
    /// Face and edge rules declared once and used by name through
    /// `FaceSelector::Named` and `EdgeSelector::Named`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<NamedReference>,
    /// Linear RGB colors, channels in [0, 1], by feature id: a feature
    /// colors the faces it creates, and later features carry those colors
    /// to the faces they keep or modify. Exported as STEP face colors.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub feature_colors: BTreeMap<String, [f64; 3]>,
    /// Engineering assumptions that requirements trace to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<Assumption>,
}

/// A face or edge rule declared once in a family and used by name, so the
/// rule (often a persistent reference) is written and edited in one place.
/// A reference's rule cannot itself use named references.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NamedReference {
    pub name: String,
    pub target: ReferenceTarget,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceTarget {
    Faces(FaceSelector),
    Edges(EdgeSelector),
}

/// Whether a named reference is used where faces or edges are expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceUse {
    Faces,
    Edges,
}

impl ReferenceTarget {
    pub(crate) fn kind(&self) -> ReferenceUse {
        match self {
            Self::Faces(_) => ReferenceUse::Faces,
            Self::Edges(_) => ReferenceUse::Edges,
        }
    }

    pub(crate) fn dependencies<'a>(&'a self, dependencies: &mut Vec<&'a str>) {
        match self {
            Self::Faces(selector) => selector.dependencies(dependencies),
            Self::Edges(selector) => selector.dependencies(dependencies),
        }
    }

    pub(crate) fn names<'a>(&'a self, names: &mut Vec<(&'a str, ReferenceUse)>) {
        match self {
            Self::Faces(selector) => selector.names(names),
            Self::Edges(selector) => selector.names(names),
        }
    }
}

/// A family's named references by name.
pub(crate) type References<'a> = HashMap<&'a str, &'a NamedReference>;

/// Indexes a family's named references by name. O(references).
pub(crate) fn reference_map(family: &FamilyDefinition) -> References<'_> {
    family
        .references
        .iter()
        .map(|reference| (reference.name.as_str(), reference))
        .collect()
}

impl FamilyDefinition {
    /// Validated, sorted input-output identities for every feature, including
    /// dependencies introduced by named references. Does not generate geometry.
    pub fn feature_inputs(
        &self,
    ) -> Result<std::collections::BTreeMap<&str, Vec<&str>>, ModelError> {
        validate_definition(self)?;
        let references = reference_map(self);
        Ok(self
            .features
            .iter()
            .map(|feature| {
                let mut inputs = dependencies_with_references(feature, &references);
                inputs.sort_unstable();
                inputs.dedup();
                (feature.id.as_str(), inputs)
            })
            .collect())
    }
}

/// Feature dependencies, including those of the named references it uses.
/// O(selectors + used references).
pub(crate) fn dependencies_with_references<'a>(
    feature: &'a FeatureDefinition,
    references: &References<'a>,
) -> Vec<&'a str> {
    let mut dependencies = feature.operation.dependencies();
    for (name, _) in feature.operation.reference_names() {
        if let Some(reference) = references.get(name) {
            reference.target.dependencies(&mut dependencies);
        }
    }
    dependencies
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartInstance<'definition> {
    pub id: String,
    pub definition: &'definition FamilyDefinition,
    pub overrides: HashMap<String, ParameterValue>,
    pub provenance: String,
}
