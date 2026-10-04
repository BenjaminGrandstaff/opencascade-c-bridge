//! Assembly semantics: named datums, datum relationships, configurations,
//! and materials.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

mod collisions;
mod configurations;
mod geometry;
mod joints;
mod mass_properties;
mod materials;
mod motion;
mod requirements;

pub use collisions::{CollisionOptions, InstanceOutputRef, PairCheck, PairStatus};
pub(crate) use geometry::*;
mod linkage;
pub use linkage::{JointSolution, JointSolveOptions, JointVariable};

pub use joints::{AssemblyJoint, JointDof, JointKind, JointScalar};
pub use mass_properties::{
    AssemblyMassProperties, ComponentMassProperties, PhysicalMassProperties,
};
use materials::*;
pub use motion::{
    ContinuousCollisionOptions, ContinuousMotionResult, ContinuousPairResult, ContinuousStatus,
    JointPosition, MAX_MOTION_SAMPLES, MotionResult, MotionSample, MotionSampleResult, MotionStudy,
};

/// Linear tolerance, in millimeters, for relationship checks.
pub const RELATIONSHIP_LINEAR_TOLERANCE: f64 = 1e-6;

/// Angular tolerance, in radians, for relationship checks.
pub const RELATIONSHIP_ANGULAR_TOLERANCE: f64 = 1e-9;

/// Per-model tolerances used to solve and check assembly relationships.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelationshipTolerances {
    pub linear_millimeters: f64,
    pub angular_radians: f64,
}

impl Default for RelationshipTolerances {
    fn default() -> Self {
        Self {
            linear_millimeters: RELATIONSHIP_LINEAR_TOLERANCE,
            angular_radians: RELATIONSHIP_ANGULAR_TOLERANCE,
        }
    }
}

impl RelationshipTolerances {
    fn validate(self) -> Result<(), ModelError> {
        if !(self.linear_millimeters.is_finite() && self.linear_millimeters > 0.0) {
            return Err(ModelError::new(
                "relationship linear tolerance must be finite and positive",
            ));
        }
        if !(self.angular_radians.is_finite() && self.angular_radians > 0.0) {
            return Err(ModelError::new(
                "relationship angular tolerance must be finite and positive",
            ));
        }
        Ok(())
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A named reference point, axis, or plane defined in a family's local
/// coordinates from parameter expressions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatumDefinition {
    pub id: String,
    pub kind: DatumKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatumKind {
    Point {
        origin: VectorExpr,
    },
    Axis {
        origin: VectorExpr,
        direction: VectorExpr,
    },
    Plane {
        origin: VectorExpr,
        normal: VectorExpr,
    },
}

/// A datum in model coordinates: millimeters and unit directions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResolvedDatum {
    Point { origin: Vec3 },
    Axis { origin: Vec3, direction: Vec3 },
    Plane { origin: Vec3, normal: Vec3 },
}

impl DatumKind {
    pub(crate) fn evaluate(
        &self,
        parameters: &HashMap<String, ParameterValue>,
    ) -> Result<ResolvedDatum, ModelError> {
        let point = |expression| vector(expression, parameters, Dimension::Length);
        let direction =
            |expression| vector(expression, parameters, Dimension::Scalar).and_then(unit);
        Ok(match self {
            Self::Point { origin } => ResolvedDatum::Point {
                origin: point(origin)?,
            },
            Self::Axis {
                origin,
                direction: axis,
            } => ResolvedDatum::Axis {
                origin: point(origin)?,
                direction: direction(axis)?,
            },
            Self::Plane { origin, normal } => ResolvedDatum::Plane {
                origin: point(origin)?,
                normal: direction(normal)?,
            },
        })
    }
}

impl ResolvedDatum {
    pub(crate) fn transformed(self, placement: &NormalizedPlacement) -> Self {
        let point = |value| transform_point(value, placement);
        let direction = |value| rotate_by(value, placement);
        match self {
            Self::Point { origin } => Self::Point {
                origin: point(origin),
            },
            Self::Axis {
                origin,
                direction: axis,
            } => Self::Axis {
                origin: point(origin),
                direction: direction(axis),
            },
            Self::Plane { origin, normal } => Self::Plane {
                origin: point(origin),
                normal: direction(normal),
            },
        }
    }
}

/// Datum ids must be unique, and every datum must evaluate with the family
/// defaults to the right dimensions and a nonzero direction.
pub(crate) fn validate_datums(definition: &FamilyDefinition) -> Result<(), ModelError> {
    insert_unique_ids(
        &mut HashSet::new(),
        definition.datums.iter().map(|datum| datum.id.as_str()),
        "datum ids must be nonempty and unique",
    )?;
    if definition.datums.is_empty() {
        return Ok(());
    }
    let parameters = resolve_parameters(definition, &HashMap::new())?;
    for datum in &definition.datums {
        datum
            .kind
            .evaluate(&parameters)
            .map_err(|error| ModelError::new(format!("datum '{}': {}", datum.id, error.message)))?;
    }
    Ok(())
}

/// Names one datum on one instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatumRef {
    pub instance: String,
    pub datum: String,
}

impl DatumRef {
    pub fn new(instance: impl Into<String>, datum: impl Into<String>) -> Self {
        Self {
            instance: instance.into(),
            datum: datum.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// Points meet; a point lies on an axis or plane; axes are collinear;
    /// planes are coplanar; an axis lies in a plane.
    Coincident,
    /// Axes or plane normals are parallel; an axis is parallel to a plane.
    Parallel,
    /// Axes or plane normals are perpendicular; an axis is normal to a plane.
    Perpendicular,
    /// Separation between points, a point and an axis or plane, or parallel
    /// axes or planes.
    Distance(Quantity),
}

/// Design intent between two instance datums. Relationships are recorded and
/// checked against the current placements; they do not move instances.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssemblyRelationship {
    pub id: String,
    pub kind: RelationKind,
    pub first: DatumRef,
    pub second: DatumRef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelationshipCheck {
    pub id: String,
    pub satisfied: bool,
    /// Linear deviation in millimeters, when the relationship constrains position.
    pub linear_residual: Option<f64>,
    /// Angular deviation in radians, when the relationship constrains direction.
    pub angular_residual: Option<f64>,
}

/// A named variant layering parameter overrides and instance suppression
/// over the base graph without changing it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    pub id: String,
    /// Per-instance overrides applied after the instance's own overrides and
    /// inherited by its clones like ordinary overrides.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, BTreeMap<String, ParameterValue>>,
    /// Instances left out of graph regeneration in this configuration.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub suppressed: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: String,
    pub name: String,
    pub density_kg_per_cubic_meter: f64,
}

/// A graph-level rule evaluated after every requested instance is generated.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyVerificationRule {
    MassRange {
        instance: String,
        output: String,
        minimum_kilograms: f64,
        maximum_kilograms: f64,
    },
    DatumClearance {
        first: DatumRef,
        second: DatumRef,
        minimum: Quantity,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<Quantity>,
    },
    RelationshipSatisfied {
        relationship: String,
    },
}

/// Stable assembly intent with the same priority semantics as family
/// requirements, but evaluated with instance, datum, and material context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssemblyRequirement {
    pub id: String,
    pub version: u32,
    pub kind: RequirementKind,
    pub priority: RequirementPriority,
    pub statement: String,
    pub rule: AssemblyVerificationRule,
    pub provenance: String,
}

/// Relationships, configurations, and materials of one instance graph.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AssemblySemantics {
    /// Tolerances used by every relationship in this model.
    #[serde(default, skip_serializing_if = "RelationshipTolerances::is_default")]
    pub tolerances: RelationshipTolerances,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<AssemblyRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<AssemblyRelationship>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub configurations: Vec<Configuration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<Material>,
    /// Frame id to joint; an absent entry retains ordinary rest placement.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub joints: BTreeMap<String, AssemblyJoint>,
    /// Instance id to material id. Clones inherit their source's material.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub material_assignments: BTreeMap<String, String>,
    /// Optional glTF appearance, keyed by a declared material id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub material_appearances: BTreeMap<String, MaterialAppearance>,
    /// Evaluation state only; documents always describe the base graph.
    #[serde(skip)]
    pub(crate) active_configuration: Option<String>,
}

impl AssemblySemantics {
    fn configuration(&self, id: &str) -> Option<&Configuration> {
        self.configurations
            .iter()
            .find(|configuration| configuration.id == id)
    }

    fn active(&self) -> Option<&Configuration> {
        self.active_configuration
            .as_deref()
            .and_then(|id| self.configuration(id))
    }

    /// Overrides the active configuration adds to one instance.
    pub(crate) fn configured_overrides(
        &self,
        instance: &str,
    ) -> Option<&BTreeMap<String, ParameterValue>> {
        self.active()?.overrides.get(instance)
    }

    pub(crate) fn configuration_suppresses(&self, instance: &str) -> bool {
        self.active()
            .is_some_and(|configuration| configuration.suppressed.contains(instance))
    }

    /// What in the assembly semantics names this instance, if anything.
    pub(crate) fn reference_to(&self, instance: &str) -> Option<String> {
        if let Some(requirement) =
            self.requirements
                .iter()
                .find(|requirement| match &requirement.rule {
                    AssemblyVerificationRule::MassRange {
                        instance: target, ..
                    } => target == instance,
                    AssemblyVerificationRule::DatumClearance { first, second, .. } => {
                        first.instance == instance || second.instance == instance
                    }
                    AssemblyVerificationRule::RelationshipSatisfied { .. } => false,
                })
        {
            return Some(format!("assembly requirement '{}'", requirement.id));
        }
        if let Some(relationship) = self.relationships.iter().find(|relationship| {
            relationship.first.instance == instance || relationship.second.instance == instance
        }) {
            return Some(format!("relationship '{}'", relationship.id));
        }
        if let Some(configuration) = self.configurations.iter().find(|configuration| {
            configuration.overrides.contains_key(instance)
                || configuration.suppressed.contains(instance)
        }) {
            return Some(format!("configuration '{}'", configuration.id));
        }
        self.material_assignments
            .contains_key(instance)
            .then(|| "a material assignment".to_owned())
    }
}

impl<'definition> InstanceGraph<'definition> {
    pub fn assembly(&self) -> &AssemblySemantics {
        &self.assembly
    }

    pub fn relationship_tolerances(&self) -> RelationshipTolerances {
        self.assembly.tolerances
    }

    /// Changes the tolerances used to solve and check every relationship.
    pub fn set_relationship_tolerances(
        &mut self,
        tolerances: RelationshipTolerances,
    ) -> Result<(), ModelError> {
        tolerances.validate()?;
        self.assembly.tolerances = tolerances;
        Ok(())
    }

    // ---- datums

    /// A family datum of an instance in model coordinates, after the
    /// instance's parameters, placement, and enclosing frames.
    pub fn datum(&self, instance: &str, datum: &str) -> Result<ResolvedDatum, ModelError> {
        let resolved = self.resolve_with_placement(instance)?;
        let definition = resolved.instance.definition;
        let declaration = definition
            .datums
            .iter()
            .find(|candidate| candidate.id == datum)
            .ok_or_else(|| {
                ModelError::new(format!("unknown datum '{datum}' on instance '{instance}'"))
            })?;
        let parameters = resolve_parameters(definition, &resolved.instance.overrides)?;
        let mut value = declaration.kind.evaluate(&parameters)?;
        for placement in std::iter::once(resolved.placement).chain(resolved.frames) {
            value = value.transformed(&placement.normalized()?);
        }
        Ok(value)
    }

    // ---- relationships

    /// Records a relationship after checking that both datums resolve and the
    /// relationship applies to their kinds.
    pub fn add_relationship(
        &mut self,
        relationship: AssemblyRelationship,
    ) -> Result<(), ModelError> {
        if relationship.id.is_empty()
            || self
                .assembly
                .relationships
                .iter()
                .any(|existing| existing.id == relationship.id)
        {
            return Err(ModelError::new(
                "relationship ids must be nonempty and unique",
            ));
        }
        self.check_relationship(&relationship)?;
        self.assembly.relationships.push(relationship);
        Ok(())
    }

    pub fn remove_relationship(&mut self, id: &str) -> Result<AssemblyRelationship, ModelError> {
        if let Some(requirement) = self.assembly.requirements.iter().find(|requirement| {
            matches!(
                &requirement.rule,
                AssemblyVerificationRule::RelationshipSatisfied { relationship }
                    if relationship == id
            )
        }) {
            return Err(ModelError::new(format!(
                "relationship '{id}' is referenced by assembly requirement '{}'",
                requirement.id
            )));
        }
        let index = self
            .assembly
            .relationships
            .iter()
            .position(|relationship| relationship.id == id)
            .ok_or_else(|| ModelError::new(format!("unknown relationship '{id}'")))?;
        Ok(self.assembly.relationships.remove(index))
    }

    /// Checks every relationship against current placements and parameters,
    /// including those of the active configuration.
    pub fn check_relationships(&self) -> Result<Vec<RelationshipCheck>, ModelError> {
        self.assembly
            .relationships
            .iter()
            .map(|relationship| self.check_relationship(relationship))
            .collect()
    }

    pub(crate) fn check_relationship(
        &self,
        relationship: &AssemblyRelationship,
    ) -> Result<RelationshipCheck, ModelError> {
        let context = |error: ModelError| {
            ModelError::new(format!(
                "relationship '{}': {}",
                relationship.id, error.message
            ))
        };
        let first = self
            .datum(&relationship.first.instance, &relationship.first.datum)
            .map_err(context)?;
        let second = self
            .datum(&relationship.second.instance, &relationship.second.datum)
            .map_err(context)?;
        let (linear, angular) = residuals(relationship.kind, first, second).map_err(context)?;
        Ok(RelationshipCheck {
            id: relationship.id.clone(),
            satisfied: linear
                .is_none_or(|value| value <= self.assembly.tolerances.linear_millimeters)
                && angular.is_none_or(|value| value <= self.assembly.tolerances.angular_radians),
            linear_residual: linear,
            angular_residual: angular,
        })
    }

    // ---- document validation

    /// Checks every assembly reference, configuration, and relationship.
    pub(crate) fn validate_assembly(&self) -> Result<(), ModelError> {
        self.assembly.tolerances.validate()?;
        self.validate_joints()?;
        let mut ids = HashSet::new();
        for material in &self.assembly.materials {
            validate_material(material)?;
            if !ids.insert(material.id.as_str()) {
                return Err(ModelError::new("material ids must be unique"));
            }
        }
        for (instance, material) in &self.assembly.material_assignments {
            self.require_instance(instance)?;
            self.material(material)?;
        }
        for (material, appearance) in &self.assembly.material_appearances {
            self.material(material)?;
            appearance.validate()?;
        }
        insert_unique_ids(
            &mut HashSet::new(),
            self.assembly
                .configurations
                .iter()
                .map(|configuration| configuration.id.as_str()),
            "configuration ids must be nonempty and unique",
        )?;
        for configuration in &self.assembly.configurations {
            self.validate_configuration(&configuration.id)?;
        }
        insert_unique_ids(
            &mut HashSet::new(),
            self.assembly
                .relationships
                .iter()
                .map(|relationship| relationship.id.as_str()),
            "relationship ids must be nonempty and unique",
        )?;
        for relationship in &self.assembly.relationships {
            self.check_relationship(relationship)?;
        }
        insert_unique_ids(
            &mut HashSet::new(),
            self.assembly
                .requirements
                .iter()
                .map(|requirement| requirement.id.as_str()),
            "assembly requirement ids must be nonempty, versioned, and unique",
        )?;
        for requirement in &self.assembly.requirements {
            self.validate_assembly_requirement(requirement)?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;
