//! Assembly semantics: named datums, datum relationships, configurations,
//! and materials.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// Linear tolerance, in millimeters, for relationship checks.
pub const RELATIONSHIP_LINEAR_TOLERANCE: f64 = 1e-6;
/// Angular tolerance, in radians, for relationship checks.
pub const RELATIONSHIP_ANGULAR_TOLERANCE: f64 = 1e-9;

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

/// Relationships, configurations, and materials of one instance graph.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AssemblySemantics {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<AssemblyRelationship>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub configurations: Vec<Configuration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<Material>,
    /// Instance id to material id. Clones inherit their source's material.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub material_assignments: BTreeMap<String, String>,
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
            satisfied: linear.is_none_or(|value| value <= RELATIONSHIP_LINEAR_TOLERANCE)
                && angular.is_none_or(|value| value <= RELATIONSHIP_ANGULAR_TOLERANCE),
            linear_residual: linear,
            angular_residual: angular,
        })
    }

    // ---- configurations

    pub fn add_configuration(&mut self, id: impl Into<String>) -> Result<(), ModelError> {
        let id = id.into();
        if id.is_empty() || self.assembly.configuration(&id).is_some() {
            return Err(ModelError::new(
                "configuration ids must be nonempty and unique",
            ));
        }
        self.assembly.configurations.push(Configuration {
            id,
            ..Configuration::default()
        });
        Ok(())
    }

    /// Sets an override in a configuration. Every instance must still resolve
    /// under that configuration, or the change is rejected.
    pub fn set_configuration_override(
        &mut self,
        configuration: &str,
        instance: &str,
        parameter: impl Into<String>,
        value: ParameterValue,
    ) -> Result<(), ModelError> {
        self.require_instance(instance)?;
        let parameter = parameter.into();
        let mut candidate = self.clone();
        candidate
            .configuration_mut(configuration)?
            .overrides
            .entry(instance.to_owned())
            .or_default()
            .insert(parameter, value);
        candidate.validate_configuration(configuration)?;
        self.assembly = candidate.assembly;
        Ok(())
    }

    pub fn remove_configuration_override(
        &mut self,
        configuration: &str,
        instance: &str,
        parameter: &str,
    ) -> Result<Option<ParameterValue>, ModelError> {
        let entry = self.configuration_mut(configuration)?;
        let removed = entry
            .overrides
            .get_mut(instance)
            .and_then(|overrides| overrides.remove(parameter));
        entry.overrides.retain(|_, overrides| !overrides.is_empty());
        Ok(removed)
    }

    pub fn set_configuration_suppressed(
        &mut self,
        configuration: &str,
        instance: &str,
        suppressed: bool,
    ) -> Result<(), ModelError> {
        self.require_instance(instance)?;
        let entry = self.configuration_mut(configuration)?;
        if suppressed {
            entry.suppressed.insert(instance.to_owned());
        } else {
            entry.suppressed.remove(instance);
        }
        Ok(())
    }

    /// Evaluates the graph under a configuration, or the base graph with
    /// `None`. Resolution, datums, relationship checks, pattern drivers, and
    /// graph regeneration all follow the active configuration.
    pub fn set_active_configuration(
        &mut self,
        configuration: Option<&str>,
    ) -> Result<(), ModelError> {
        if let Some(id) = configuration
            && self.assembly.configuration(id).is_none()
        {
            return Err(ModelError::new(format!("unknown configuration '{id}'")));
        }
        self.assembly.active_configuration = configuration.map(str::to_owned);
        Ok(())
    }

    pub fn active_configuration(&self) -> Option<&str> {
        self.assembly.active_configuration.as_deref()
    }

    fn configuration_mut(&mut self, id: &str) -> Result<&mut Configuration, ModelError> {
        self.assembly
            .configurations
            .iter_mut()
            .find(|configuration| configuration.id == id)
            .ok_or_else(|| ModelError::new(format!("unknown configuration '{id}'")))
    }

    fn require_instance(&self, instance: &str) -> Result<(), ModelError> {
        if self.nodes.contains_key(instance) {
            Ok(())
        } else {
            Err(ModelError::new(format!("unknown instance '{instance}'")))
        }
    }

    /// Every instance must resolve valid parameters under the configuration.
    fn validate_configuration(&self, id: &str) -> Result<(), ModelError> {
        let configuration = self
            .assembly
            .configuration(id)
            .ok_or_else(|| ModelError::new(format!("unknown configuration '{id}'")))?;
        if let Some(instance) = configuration
            .overrides
            .keys()
            .chain(&configuration.suppressed)
            .find(|instance| !self.nodes.contains_key(instance.as_str()))
        {
            return Err(ModelError::new(format!(
                "configuration '{id}' references unknown instance '{instance}'"
            )));
        }
        let mut configured = self.clone();
        configured.assembly.active_configuration = Some(id.to_owned());
        let mut resolutions = HashMap::new();
        for instance in self.nodes.keys() {
            let resolved = configured.resolve_cached(instance, &mut resolutions)?;
            resolve_parameters(resolved.definition, &resolved.overrides).map_err(|error| {
                ModelError::new(format!(
                    "configuration '{id}' instance '{instance}': {}",
                    error.message
                ))
            })?;
        }
        Ok(())
    }

    // ---- materials

    pub fn add_material(&mut self, material: Material) -> Result<(), ModelError> {
        validate_material(&material)?;
        if self
            .assembly
            .materials
            .iter()
            .any(|existing| existing.id == material.id)
        {
            return Err(ModelError::new("material ids must be unique"));
        }
        self.assembly.materials.push(material);
        Ok(())
    }

    /// Assigns a material to an instance, or clears its own assignment so it
    /// inherits from its clone source again.
    pub fn assign_material(
        &mut self,
        instance: &str,
        material: Option<&str>,
    ) -> Result<(), ModelError> {
        self.require_instance(instance)?;
        match material {
            Some(id) => {
                self.material(id)?;
                self.assembly
                    .material_assignments
                    .insert(instance.to_owned(), id.to_owned());
            }
            None => {
                self.assembly.material_assignments.remove(instance);
            }
        }
        Ok(())
    }

    fn material(&self, id: &str) -> Result<&Material, ModelError> {
        self.assembly
            .materials
            .iter()
            .find(|material| material.id == id)
            .ok_or_else(|| ModelError::new(format!("unknown material '{id}'")))
    }

    /// The instance's own material, or the nearest one along its clone sources.
    pub fn material_of(&self, instance: &str) -> Result<Option<&Material>, ModelError> {
        let mut current = instance;
        for _ in 0..=self.nodes.len() {
            if let Some(id) = self.assembly.material_assignments.get(current) {
                return self.material(id).map(Some);
            }
            match self.nodes.get(current) {
                Some(InstanceNode::Clone { source, .. }) => current = source,
                Some(InstanceNode::Base { .. }) => return Ok(None),
                None => return Err(ModelError::new(format!("unknown instance '{current}'"))),
            }
        }
        Err(ModelError::new(format!(
            "clone cycle while resolving material of '{instance}'"
        )))
    }

    /// Mass in kilograms of one generated output: its volume times the
    /// instance's material density.
    pub fn mass(&self, session: &Session, instance: &str, output: &str) -> Result<f64, ModelError> {
        let density = self
            .material_of(instance)?
            .ok_or_else(|| ModelError::new(format!("instance '{instance}' has no material")))?
            .density_kg_per_cubic_meter;
        let generated = self.resolve(instance)?.regenerate(session)?;
        let volume = generated
            .shape(output)
            .ok_or_else(|| {
                ModelError::new(format!("instance '{instance}' has no output '{output}'"))
            })
            .and_then(|shape| session.volume(shape).map_err(ModelError::from));
        cleanup(session, generated.shapes);
        Ok(volume? * CUBIC_MILLIMETERS_TO_CUBIC_METERS * density)
    }

    /// Keeps an inherited material when an instance stops inheriting.
    pub(crate) fn pin_material(&mut self, instance: &str, inherited: Option<String>) {
        if let Some(material) = inherited {
            self.assembly
                .material_assignments
                .entry(instance.to_owned())
                .or_insert(material);
        }
    }

    // ---- document validation

    /// Checks every assembly reference, configuration, and relationship.
    pub(crate) fn validate_assembly(&self) -> Result<(), ModelError> {
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
        Ok(())
    }
}

const CUBIC_MILLIMETERS_TO_CUBIC_METERS: f64 = 1e-9;

fn validate_material(material: &Material) -> Result<(), ModelError> {
    if material.id.is_empty() || material.name.is_empty() {
        return Err(ModelError::new("material id and name are required"));
    }
    if !(material.density_kg_per_cubic_meter.is_finite()
        && material.density_kg_per_cubic_meter > 0.0)
    {
        return Err(ModelError::new(format!(
            "material '{}' density must be finite and positive",
            material.id
        )));
    }
    Ok(())
}

// ---- relationship geometry

type Residuals = (Option<f64>, Option<f64>);

fn residuals(
    kind: RelationKind,
    first: ResolvedDatum,
    second: ResolvedDatum,
) -> Result<Residuals, ModelError> {
    match kind {
        RelationKind::Coincident => coincident(first, second),
        RelationKind::Parallel => directional(first, second, false),
        RelationKind::Perpendicular => directional(first, second, true),
        RelationKind::Distance(value) => {
            if value.dimension != Dimension::Length {
                return Err(ModelError::new("relationship distance must be a length"));
            }
            let target = value.normalized()?;
            if target.is_nan() || target < 0.0 {
                return Err(ModelError::new("relationship distance must be nonnegative"));
            }
            let (measured, angular) = separation(first, second)?;
            Ok((Some((measured - target).abs()), angular))
        }
    }
}

fn coincident(first: ResolvedDatum, second: ResolvedDatum) -> Result<Residuals, ModelError> {
    use ResolvedDatum::{Axis, Plane};
    Ok(match (first, second) {
        (
            Axis { origin, direction },
            Plane {
                origin: plane,
                normal,
            },
        )
        | (
            Plane {
                origin: plane,
                normal,
            },
            Axis { origin, direction },
        ) => (
            Some(dot(subtract(origin, plane), normal).abs()),
            Some(perpendicular_angle(direction, normal)),
        ),
        _ => separation(first, second).map(|(linear, angular)| (Some(linear), angular))?,
    })
}

/// Distance between two datums, with the angular deviation from parallel
/// for axis and plane pairs whose distance is only defined when parallel.
fn separation(
    first: ResolvedDatum,
    second: ResolvedDatum,
) -> Result<(f64, Option<f64>), ModelError> {
    use ResolvedDatum::{Axis, Plane, Point};
    Ok(match (first, second) {
        (Point { origin: p }, Point { origin: q }) => (length(subtract(p, q)), None),
        (Point { origin: p }, Axis { origin, direction })
        | (Axis { origin, direction }, Point { origin: p }) => {
            (length(cross(subtract(p, origin), direction)), None)
        }
        (Point { origin: p }, Plane { origin, normal })
        | (Plane { origin, normal }, Point { origin: p }) => {
            (dot(subtract(p, origin), normal).abs(), None)
        }
        (
            Axis {
                origin: a,
                direction: d,
            },
            Axis {
                origin: b,
                direction: e,
            },
        ) => (length(cross(subtract(b, a), d)), Some(parallel_angle(d, e))),
        (
            Plane {
                origin: a,
                normal: n,
            },
            Plane {
                origin: b,
                normal: m,
            },
        ) => (dot(subtract(b, a), n).abs(), Some(parallel_angle(n, m))),
        (Axis { .. }, Plane { .. }) | (Plane { .. }, Axis { .. }) => {
            return Err(ModelError::new(
                "distance between an axis and a plane is not supported; use coincident",
            ));
        }
    })
}

/// Parallel or perpendicular intent between axis directions and plane
/// normals. An axis is parallel to a plane when it is perpendicular to the
/// plane normal, and the reverse.
fn directional(
    first: ResolvedDatum,
    second: ResolvedDatum,
    perpendicular: bool,
) -> Result<Residuals, ModelError> {
    let direction = |datum| match datum {
        ResolvedDatum::Axis { direction, .. } => Ok((direction, false)),
        ResolvedDatum::Plane { normal, .. } => Ok((normal, true)),
        ResolvedDatum::Point { .. } => Err(ModelError::new(
            "parallel and perpendicular relationships require axes or planes",
        )),
    };
    let (u, first_is_plane) = direction(first)?;
    let (v, second_is_plane) = direction(second)?;
    // For a mixed axis/plane pair the plane normal flips the sense.
    let wants_perpendicular = perpendicular != (first_is_plane != second_is_plane);
    let angle = if wants_perpendicular {
        perpendicular_angle(u, v)
    } else {
        parallel_angle(u, v)
    };
    Ok((None, Some(angle)))
}

fn parallel_angle(u: Vec3, v: Vec3) -> f64 {
    length(cross(u, v)).atan2(dot(u, v).abs())
}

fn perpendicular_angle(u: Vec3, v: Vec3) -> f64 {
    dot(u, v).abs().atan2(length(cross(u, v)))
}

// ---- vector math

pub(crate) fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

pub(crate) fn subtract(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

pub(crate) fn scale(a: Vec3, factor: f64) -> Vec3 {
    Vec3::new(a.x * factor, a.y * factor, a.z * factor)
}

pub(crate) fn length(a: Vec3) -> f64 {
    a.x.hypot(a.y.hypot(a.z))
}

pub(crate) fn unit(a: Vec3) -> Result<Vec3, ModelError> {
    let size = length(a);
    if size > f64::EPSILON {
        Ok(scale(a, 1.0 / size))
    } else {
        Err(ModelError::new("datum direction must be nonzero"))
    }
}

/// Rodrigues rotation of a direction by the placement's axis-angle, if any.
pub(crate) fn rotate_by(value: Vec3, placement: &NormalizedPlacement) -> Vec3 {
    let Some((_, axis, angle)) = placement.rotation else {
        return value;
    };
    let axis = scale(axis, 1.0 / length(axis));
    let (sin, cos) = angle.sin_cos();
    add(
        add(scale(value, cos), scale(cross(axis, value), sin)),
        scale(axis, dot(axis, value) * (1.0 - cos)),
    )
}

/// Rotates a point about the placement's axis through its origin, then translates.
pub(crate) fn transform_point(point: Vec3, placement: &NormalizedPlacement) -> Vec3 {
    let rotated = match placement.rotation {
        Some((origin, _, _)) => add(origin, rotate_by(subtract(point, origin), placement)),
        None => point,
    };
    add(rotated, placement.translation)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn length_parameter(id: &str, default: f64) -> ParameterDefinition {
        ParameterDefinition {
            id: id.into(),
            parameter_type: ParameterType::Scalar(Dimension::Length),
            default: ParameterValue::Scalar(Quantity::length(default, LengthUnit::Millimeter)),
            minimum: Some(Quantity::length(1.0, LengthUnit::Millimeter)),
            maximum: None,
        }
    }

    fn parameter(id: &str) -> ScalarExpr {
        ScalarExpr::Parameter(id.into())
    }

    fn half(id: &str) -> ScalarExpr {
        ScalarExpr::Multiply(
            Box::new(parameter(id)),
            Box::new(ScalarExpr::Literal(Quantity::scalar(0.5))),
        )
    }

    fn zero() -> ScalarExpr {
        ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter))
    }

    fn point(x: ScalarExpr, y: ScalarExpr, z: ScalarExpr) -> VectorExpr {
        VectorExpr::Components { x, y, z }
    }

    fn direction(x: f64, y: f64, z: f64) -> VectorExpr {
        VectorExpr::Literal(VectorQuantity::scalars(x, y, z))
    }

    /// A width x depth x height block with datums on its top, bottom, right
    /// face, top center, and vertical center axis.
    pub(crate) fn block() -> FamilyDefinition {
        FamilyDefinition {
            id: "Block".into(),
            version: 1,
            parameters: vec![
                length_parameter("width", 10.0),
                length_parameter("depth", 20.0),
                length_parameter("height", 30.0),
            ],
            derived_parameters: Vec::new(),
            derived_vector_parameters: Vec::new(),
            constraints: Vec::new(),
            features: vec![FeatureDefinition {
                id: "body".into(),
                operation: FeatureOperation::Box {
                    origin: VectorExpr::Literal(VectorQuantity::lengths(
                        0.0,
                        0.0,
                        0.0,
                        LengthUnit::Millimeter,
                    )),
                    size: point(parameter("width"), parameter("depth"), parameter("height")),
                },
            }],
            requirements: Vec::new(),
            datums: vec![
                DatumDefinition {
                    id: "top_center".into(),
                    kind: DatumKind::Point {
                        origin: point(half("width"), half("depth"), parameter("height")),
                    },
                },
                DatumDefinition {
                    id: "axis".into(),
                    kind: DatumKind::Axis {
                        origin: point(half("width"), half("depth"), zero()),
                        direction: direction(0.0, 0.0, 1.0),
                    },
                },
                DatumDefinition {
                    id: "top".into(),
                    kind: DatumKind::Plane {
                        origin: point(zero(), zero(), parameter("height")),
                        normal: direction(0.0, 0.0, 1.0),
                    },
                },
                DatumDefinition {
                    id: "bottom".into(),
                    kind: DatumKind::Plane {
                        origin: point(zero(), zero(), zero()),
                        normal: direction(0.0, 0.0, -1.0),
                    },
                },
                DatumDefinition {
                    id: "right".into(),
                    kind: DatumKind::Plane {
                        origin: point(parameter("width"), zero(), zero()),
                        normal: direction(1.0, 0.0, 0.0),
                    },
                },
            ],
        }
    }

    fn width(millimeters: f64) -> ParameterValue {
        ParameterValue::Scalar(Quantity::length(millimeters, LengthUnit::Millimeter))
    }

    pub(crate) fn translated(x: f64, y: f64, z: f64) -> Placement {
        Placement::translated(VectorQuantity::lengths(x, y, z, LengthUnit::Millimeter))
    }

    pub(crate) fn assert_point(datum: ResolvedDatum, expected: (f64, f64, f64)) {
        let ResolvedDatum::Point { origin } = datum else {
            panic!("expected a point, got {datum:?}");
        };
        let error = length(subtract(
            origin,
            Vec3::new(expected.0, expected.1, expected.2),
        ));
        assert!(error < 1e-9, "{origin:?} != {expected:?}");
    }

    /// Block `a` at the origin and its clone `b` stacked on top of it.
    pub(crate) fn stacked(definition: &FamilyDefinition) -> InstanceGraph<'_> {
        let mut graph = InstanceGraph::new(definition);
        graph.add_base("a", HashMap::new(), "test").unwrap();
        graph.add_clone("b", "a", HashMap::new(), "test").unwrap();
        graph
            .set_placement("b", translated(0.0, 0.0, 30.0))
            .unwrap();
        graph
    }

    pub(crate) fn relationship(
        id: &str,
        kind: RelationKind,
        first: (&str, &str),
        second: (&str, &str),
    ) -> AssemblyRelationship {
        AssemblyRelationship {
            id: id.into(),
            kind,
            first: DatumRef::new(first.0, first.1),
            second: DatumRef::new(second.0, second.1),
        }
    }

    #[test]
    fn datums_follow_parameters_placement_and_frames() {
        let definition = block();
        let mut graph = InstanceGraph::new(&definition);
        graph
            .add_base("a", HashMap::from([("width".into(), width(40.0))]), "test")
            .unwrap();
        assert_point(graph.datum("a", "top_center").unwrap(), (20.0, 10.0, 30.0));

        // A quarter turn about Z in a frame, after a local translation.
        graph
            .add_frame(
                "turned",
                None,
                Placement {
                    translation: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                    rotation: Some(AxisAngle {
                        origin: VectorQuantity::lengths(0.0, 0.0, 0.0, LengthUnit::Millimeter),
                        axis: VectorQuantity::scalars(0.0, 0.0, 1.0),
                        angle_radians: FRAC_PI_2,
                    }),
                },
                "layout",
            )
            .unwrap();
        graph
            .set_placement("a", translated(100.0, 0.0, 0.0))
            .unwrap();
        graph.set_instance_frame("a", Some("turned")).unwrap();
        assert_point(
            graph.datum("a", "top_center").unwrap(),
            (-10.0, 120.0, 30.0),
        );
        let ResolvedDatum::Plane { normal, .. } = graph.datum("a", "right").unwrap() else {
            panic!("right is a plane");
        };
        assert!(length(subtract(normal, Vec3::new(0.0, 1.0, 0.0))) < 1e-12);

        assert!(
            graph
                .datum("a", "missing")
                .unwrap_err()
                .message
                .contains("unknown datum")
        );

        let mut flat = block();
        flat.datums[1].kind = DatumKind::Axis {
            origin: point(zero(), zero(), zero()),
            direction: direction(0.0, 0.0, 0.0),
        };
        assert!(
            validate_datums(&flat)
                .unwrap_err()
                .message
                .contains("nonzero")
        );
        let mut untyped = block();
        untyped.datums[0].kind = DatumKind::Point {
            origin: direction(1.0, 0.0, 0.0),
        };
        assert!(validate_datums(&untyped).is_err());
        let mut duplicated = block();
        duplicated.datums.push(duplicated.datums[0].clone());
        assert!(validate_datums(&duplicated).is_err());
    }

    #[test]
    fn relationships_report_satisfied_and_violated_intent() {
        let definition = block();
        let mut graph = stacked(&definition);
        let distance = |millimeters| {
            RelationKind::Distance(Quantity::length(millimeters, LengthUnit::Millimeter))
        };
        let cases = [
            relationship(
                "seated",
                RelationKind::Coincident,
                ("a", "top"),
                ("b", "bottom"),
            ),
            relationship(
                "aligned",
                RelationKind::Coincident,
                ("a", "axis"),
                ("b", "axis"),
            ),
            relationship(
                "centered",
                RelationKind::Coincident,
                ("a", "top_center"),
                ("b", "axis"),
            ),
            relationship("level", RelationKind::Parallel, ("a", "top"), ("b", "top")),
            relationship(
                "along_face",
                RelationKind::Parallel,
                ("a", "axis"),
                ("a", "right"),
            ),
            relationship(
                "upright",
                RelationKind::Perpendicular,
                ("a", "axis"),
                ("a", "top"),
            ),
            relationship(
                "square",
                RelationKind::Perpendicular,
                ("a", "right"),
                ("b", "top"),
            ),
            relationship(
                "stack",
                distance(30.0),
                ("a", "top_center"),
                ("b", "top_center"),
            ),
            relationship("gap", distance(30.0), ("a", "top"), ("b", "top")),
            relationship(
                "in_top",
                RelationKind::Coincident,
                ("b", "axis"),
                ("a", "right"),
            ),
        ];
        for case in &cases[..9] {
            graph.add_relationship(case.clone()).unwrap();
        }
        let checks = graph.check_relationships().unwrap();
        assert!(checks.iter().all(|check| check.satisfied), "{checks:?}");

        // An axis coincident with a plane must lie in it; the vertical axis
        // does not lie in the right face.
        graph.add_relationship(cases[9].clone()).unwrap();
        let in_top = graph.check_relationships().unwrap().pop().unwrap();
        assert!(!in_top.satisfied);
        assert!((in_top.linear_residual.unwrap() - 5.0).abs() < 1e-9);

        graph
            .set_placement("b", translated(0.0, 0.0, 31.0))
            .unwrap();
        let checks = graph.check_relationships().unwrap();
        let seated = checks.iter().find(|check| check.id == "seated").unwrap();
        assert!(!seated.satisfied);
        assert!((seated.linear_residual.unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(seated.angular_residual, Some(0.0));
        assert!(
            checks
                .iter()
                .find(|check| check.id == "aligned")
                .unwrap()
                .satisfied
        );

        let rejected = [
            relationship(
                "seated",
                RelationKind::Coincident,
                ("a", "top"),
                ("b", "bottom"),
            ),
            relationship(
                "points",
                RelationKind::Parallel,
                ("a", "top_center"),
                ("b", "top"),
            ),
            relationship("mixed", distance(1.0), ("a", "axis"), ("b", "top")),
            relationship(
                "scalar",
                RelationKind::Distance(Quantity::scalar(1.0)),
                ("a", "top"),
                ("b", "top"),
            ),
            relationship(
                "unknown",
                RelationKind::Coincident,
                ("a", "missing"),
                ("b", "top"),
            ),
            relationship(
                "nobody",
                RelationKind::Coincident,
                ("z", "top"),
                ("b", "top"),
            ),
        ];
        for case in rejected {
            assert!(graph.add_relationship(case.clone()).is_err(), "{case:?}");
        }
        assert_eq!(graph.remove_relationship("in_top").unwrap().id, "in_top");
        assert!(graph.remove_relationship("in_top").is_err());
    }

    #[test]
    fn configurations_layer_overrides_and_suppression_over_the_base_graph() {
        let definition = block();
        let mut graph = stacked(&definition);
        graph.add_configuration("wide").unwrap();
        graph.add_configuration("single").unwrap();
        assert!(graph.add_configuration("wide").is_err());
        graph
            .set_configuration_override("wide", "a", "width", width(40.0))
            .unwrap();
        graph
            .set_configuration_suppressed("single", "b", true)
            .unwrap();

        // Invalid overrides are rejected without changing the configuration.
        assert!(
            graph
                .set_configuration_override("wide", "a", "width", width(0.5))
                .is_err()
        );
        assert!(
            graph
                .set_configuration_override("wide", "missing", "width", width(5.0))
                .is_err()
        );
        assert!(graph.set_active_configuration(Some("missing")).is_err());

        assert_point(graph.datum("b", "top_center").unwrap(), (5.0, 10.0, 60.0));
        graph.set_active_configuration(Some("wide")).unwrap();
        assert_eq!(graph.active_configuration(), Some("wide"));
        // The clone inherits the configured width of its source.
        assert_point(graph.datum("b", "top_center").unwrap(), (20.0, 10.0, 60.0));

        let session = Session::new().unwrap();
        graph.set_active_configuration(Some("single")).unwrap();
        assert!(graph.is_suppressed("b"));
        let generation = graph.regenerate_all(&session).unwrap();
        assert!(generation.result("a").is_some() && generation.result("b").is_none());
        drop(generation);

        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
        let reloaded = loaded.instance_graph().unwrap();
        assert_eq!(reloaded.active_configuration(), None);
        assert!(!reloaded.is_suppressed("b"));

        graph.set_active_configuration(None).unwrap();
        assert_point(graph.datum("b", "top_center").unwrap(), (5.0, 10.0, 60.0));
        assert_eq!(
            graph
                .remove_configuration_override("wide", "a", "width")
                .unwrap(),
            Some(width(40.0))
        );
        assert!(graph.assembly().configurations[0].overrides.is_empty());
    }

    #[test]
    fn materials_are_inherited_and_give_mass() {
        let definition = block();
        let mut graph = stacked(&definition);
        let steel = Material {
            id: "steel".into(),
            name: "Structural steel".into(),
            density_kg_per_cubic_meter: 7850.0,
        };
        graph.add_material(steel.clone()).unwrap();
        assert!(graph.add_material(steel.clone()).is_err());
        assert!(
            graph
                .add_material(Material {
                    density_kg_per_cubic_meter: 0.0,
                    id: "void".into(),
                    ..steel.clone()
                })
                .is_err()
        );
        assert!(graph.assign_material("a", Some("unobtainium")).is_err());

        graph.assign_material("a", Some("steel")).unwrap();
        assert_eq!(graph.material_of("b").unwrap(), Some(&steel));
        let session = Session::new().unwrap();
        // 10 x 20 x 30 mm is 6e-6 m^3.
        let mass = graph.mass(&session, "b", "body").unwrap();
        assert!((mass - 6e-6 * 7850.0).abs() < 1e-12);
        assert_eq!(session.shape_count().unwrap(), 0);
        assert!(graph.mass(&session, "b", "missing").is_err());

        // Detaching keeps the material the clone used to inherit.
        graph.detach("b").unwrap();
        graph.assign_material("a", None).unwrap();
        assert_eq!(graph.material_of("a").unwrap(), None);
        assert_eq!(graph.material_of("b").unwrap(), Some(&steel));
        assert!(
            graph
                .mass(&session, "a", "body")
                .unwrap_err()
                .message
                .contains("no material")
        );

        let document = ModelDocument::from_graph(&graph);
        let loaded = ModelDocument::from_json(&document.to_json_pretty().unwrap()).unwrap();
        assert_eq!(loaded, document);
    }

    #[test]
    fn referenced_pattern_members_are_not_deleted_and_documents_reject_dangling_references() {
        let definition = block();
        let mut graph = InstanceGraph::new(&definition);
        graph.add_base("a", HashMap::new(), "test").unwrap();
        graph
            .add_linear_pattern(
                "row",
                "seat",
                "a",
                3,
                VectorQuantity::lengths(50.0, 0.0, 0.0, LengthUnit::Millimeter),
                "test",
            )
            .unwrap();
        graph
            .add_relationship(relationship(
                "spacing",
                RelationKind::Distance(Quantity::length(100.0, LengthUnit::Millimeter)),
                ("seat[0]", "top_center"),
                ("seat[2]", "top_center"),
            ))
            .unwrap();
        let error = graph.set_pattern_count("row", 2).unwrap_err();
        assert!(error.message.contains("relationship 'spacing'"), "{error}");
        assert_eq!(graph.patterns()[0].slot_count, 3);

        let document = ModelDocument::from_graph(&graph);
        let mut dangling = document.clone();
        dangling.assembly.relationships[0].second.instance = "gone".into();
        assert!(dangling.to_json_pretty().is_err());
        let mut unknown_material = document;
        unknown_material
            .assembly
            .material_assignments
            .insert("a".into(), "missing".into());
        assert!(unknown_material.to_json_pretty().is_err());
    }
}
