//! Feature-graph regeneration, incremental reuse, verification, and
//! result placement.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationStatus {
    Passed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationResult {
    pub requirement_id: String,
    pub status: VerificationStatus,
    pub message: String,
}

pub struct GeneratedResult<'session> {
    pub(crate) shapes: HashMap<String, Shape<'session>>,
    pub(crate) feature_signatures: HashMap<String, Vec<u8>>,
    pub verification: Vec<VerificationResult>,
    pub regeneration: RegenerationReport,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegenerationReport {
    pub rebuilt: Vec<String>,
    pub reused: Vec<String>,
}

impl<'session> GeneratedResult<'session> {
    pub fn shape(&self, name: &str) -> Option<&Shape<'session>> {
        self.shapes.get(name)
    }

    pub fn named_outputs(&self) -> impl Iterator<Item = &str> {
        self.shapes.keys().map(String::as_str)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegenerationState {
    NeverGenerated,
    Current,
    Frozen,
    Stale,
    Failed,
}

pub struct ManagedPartInstance<'definition, 'session> {
    pub(crate) session: &'session Session,
    pub(crate) instance: PartInstance<'definition>,
    pub(crate) attempted_revision: u64,
    pub(crate) accepted_revision: Option<u64>,
    pub(crate) state: RegenerationState,
    pub(crate) last_error: Option<ModelError>,
    pub(crate) accepted: Option<GeneratedResult<'session>>,
}

impl<'definition, 'session> ManagedPartInstance<'definition, 'session> {
    pub fn new(session: &'session Session, instance: PartInstance<'definition>) -> Self {
        Self {
            session,
            instance,
            attempted_revision: 0,
            accepted_revision: None,
            state: RegenerationState::NeverGenerated,
            last_error: None,
            accepted: None,
        }
    }

    pub fn instance(&self) -> &PartInstance<'definition> {
        &self.instance
    }

    pub fn instance_mut(&mut self) -> &mut PartInstance<'definition> {
        &mut self.instance
    }

    pub fn attempted_revision(&self) -> u64 {
        self.attempted_revision
    }

    pub fn accepted_revision(&self) -> Option<u64> {
        self.accepted_revision
    }

    pub fn state(&self) -> RegenerationState {
        self.state
    }

    pub fn last_error(&self) -> Option<&ModelError> {
        self.last_error.as_ref()
    }

    pub fn accepted(&self) -> Option<&GeneratedResult<'session>> {
        self.accepted.as_ref()
    }

    pub fn generation_record(&self) -> GenerationRecord {
        GenerationRecord {
            instance_id: self.instance.id.clone(),
            attempted_revision: self.attempted_revision,
            accepted_revision: self.accepted_revision,
            state: self.state,
            last_error: self.last_error.as_ref().map(ToString::to_string),
        }
    }

    pub fn freeze(&mut self) -> Result<u64, ModelError> {
        let revision = self
            .accepted_revision
            .ok_or_else(|| ModelError::new("cannot freeze before a generation is accepted"))?;
        self.state = RegenerationState::Frozen;
        Ok(revision)
    }

    pub fn unfreeze(&mut self) {
        if self.state == RegenerationState::Frozen {
            self.state = RegenerationState::Current;
        }
    }

    pub fn regenerate(&mut self) -> Result<(), ModelError> {
        if self.state == RegenerationState::Frozen {
            return Err(ModelError::new("accepted generation is frozen"));
        }
        self.attempted_revision = self.attempted_revision.saturating_add(1);
        let regeneration = match self.accepted.as_ref() {
            Some(previous) => self.instance.regenerate_incremental(self.session, previous),
            None => self.instance.regenerate(self.session),
        };
        match regeneration {
            Ok(result) => {
                if let Some(previous) = self.accepted.take() {
                    cleanup(self.session, previous.shapes);
                }
                self.accepted = Some(result);
                self.accepted_revision = Some(self.attempted_revision);
                self.state = RegenerationState::Current;
                self.last_error = None;
                Ok(())
            }
            Err(error) => {
                self.state = if self.accepted.is_some() {
                    RegenerationState::Stale
                } else {
                    RegenerationState::Failed
                };
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }
}

impl Drop for ManagedPartInstance<'_, '_> {
    fn drop(&mut self) {
        if let Some(result) = self.accepted.take() {
            cleanup(self.session, result.shapes);
        }
    }
}

impl PartInstance<'_> {
    pub fn regenerate<'session>(
        &self,
        session: &'session Session,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        self.regenerate_internal(session, None)
    }

    pub fn regenerate_incremental<'session>(
        &self,
        session: &'session Session,
        previous: &GeneratedResult<'session>,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        self.regenerate_internal(session, Some(previous))
    }

    pub(crate) fn regenerate_internal<'session>(
        &self,
        session: &'session Session,
        previous: Option<&GeneratedResult<'session>>,
    ) -> Result<GeneratedResult<'session>, ModelError> {
        validate_definition(self.definition)?;
        let parameters = resolve_parameters(self.definition, &self.overrides)?;
        let mut build = FeatureBuild::default();
        if let Err(error) = build.run(session, self.definition, &parameters, previous) {
            cleanup(session, build.shapes);
            return Err(error);
        }
        match verify_requirements(session, self.definition, &build.shapes) {
            Ok(verification) => Ok(GeneratedResult {
                shapes: build.shapes,
                feature_signatures: build.feature_signatures,
                verification,
                regeneration: build.regeneration,
            }),
            Err(error) => {
                cleanup(session, build.shapes);
                Err(error)
            }
        }
    }
}

/// Feature outputs accumulated during one regeneration. The caller releases
/// `shapes` if any step fails.
#[derive(Default)]
pub(crate) struct FeatureBuild<'session> {
    pub(crate) shapes: HashMap<String, Shape<'session>>,
    pub(crate) feature_signatures: HashMap<String, Vec<u8>>,
    pub(crate) dirty_features: HashSet<String>,
    pub(crate) regeneration: RegenerationReport,
}

impl<'session> FeatureBuild<'session> {
    /// Executes features in dependency order. Each pass runs every ready
    /// feature in declaration order; a pass without progress is a cycle or a
    /// missing input.
    pub(crate) fn run(
        &mut self,
        session: &'session Session,
        definition: &FamilyDefinition,
        parameters: &HashMap<String, ParameterValue>,
        previous: Option<&GeneratedResult<'session>>,
    ) -> Result<(), ModelError> {
        let datums = definition
            .datums
            .iter()
            .map(|datum| (datum.id.as_str(), datum))
            .collect::<HashMap<_, _>>();
        let mut pending: Vec<&FeatureDefinition> = definition.features.iter().collect();
        while !pending.is_empty() {
            let before = pending.len();
            let mut index = 0;
            while index < pending.len() {
                if self.is_ready(pending[index]) {
                    let feature = pending.remove(index);
                    self.add_feature(session, &datums, feature, parameters, previous)?;
                } else {
                    index += 1;
                }
            }
            if pending.len() == before {
                let blocked = pending
                    .iter()
                    .map(|feature| feature.id.as_str())
                    .collect::<Vec<_>>();
                return Err(ModelError::new(format!(
                    "feature dependency cycle or unresolved input: {}",
                    blocked.join(", ")
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn is_ready(&self, feature: &FeatureDefinition) -> bool {
        feature
            .operation
            .dependencies()
            .iter()
            .all(|dependency| self.shapes.contains_key(*dependency))
    }

    /// Reuses the previous output through a duplicate handle when the feature
    /// signature is unchanged and no dependency was rebuilt; otherwise executes it.
    pub(crate) fn add_feature(
        &mut self,
        session: &'session Session,
        datums: &HashMap<&str, &DatumDefinition>,
        feature: &FeatureDefinition,
        parameters: &HashMap<String, ParameterValue>,
        previous: Option<&GeneratedResult<'session>>,
    ) -> Result<(), ModelError> {
        let signature = feature_signature(datums, feature, parameters)?;
        let dependency_is_dirty = feature
            .operation
            .dependencies()
            .iter()
            .any(|dependency| self.dirty_features.contains(*dependency));
        let reusable = previous
            .filter(|previous| {
                !dependency_is_dirty
                    && previous.feature_signatures.get(&feature.id) == Some(&signature)
            })
            .and_then(|previous| previous.shapes.get(&feature.id));
        let generated = match reusable {
            Some(shape) => session.duplicate(shape).map_err(Into::into),
            None => {
                self.dirty_features.insert(feature.id.clone());
                execute_feature(session, datums, feature, parameters, &self.shapes)
            }
        };
        let shape = generated.map_err(|error| error.in_feature(&feature.id))?;
        self.shapes.insert(feature.id.clone(), shape);
        self.feature_signatures
            .insert(feature.id.clone(), signature);
        let report = if reusable.is_some() {
            &mut self.regeneration.reused
        } else {
            &mut self.regeneration.rebuilt
        };
        report.push(feature.id.clone());
        Ok(())
    }
}

/// Evaluates every requirement; required failures reject the generation.
pub(crate) fn verify_requirements(
    session: &Session,
    definition: &FamilyDefinition,
    shapes: &HashMap<String, Shape<'_>>,
) -> Result<Vec<VerificationResult>, ModelError> {
    let mut verification = Vec::new();
    let mut required_failures = Vec::new();
    for requirement in &definition.requirements {
        let result = verify_requirement(session, requirement, shapes).map_err(|error| {
            ModelError::new(format!("requirement '{}': {error}", requirement.id))
        })?;
        if requirement.priority == RequirementPriority::Required
            && result.status == VerificationStatus::Failed
        {
            required_failures.push(requirement.id.clone());
        }
        verification.push(result);
    }
    if !required_failures.is_empty() {
        return Err(ModelError::new(format!(
            "required verification failed: {}",
            required_failures.join(", ")
        )));
    }
    Ok(verification)
}

#[derive(Serialize)]
pub(crate) struct FeatureSignature<'a> {
    pub(crate) feature: &'a FeatureDefinition,
    pub(crate) parameters: Vec<(&'a str, &'a ParameterValue)>,
    pub(crate) datum: Option<&'a DatumDefinition>,
}

pub(crate) fn sketch_datum<'a>(
    datums: &HashMap<&str, &'a DatumDefinition>,
    operation: &FeatureOperation,
) -> Result<Option<&'a DatumDefinition>, ModelError> {
    let sketch = match operation {
        FeatureOperation::SketchFace { sketch } | FeatureOperation::SketchWire { sketch } => sketch,
        _ => return Ok(None),
    };
    let Some(id) = &sketch.datum_plane else {
        return Ok(None);
    };
    let datum = datums
        .get(id.as_str())
        .copied()
        .ok_or_else(|| ModelError::new(format!("unknown sketch plane datum '{id}'")))?;
    if !matches!(datum.kind, DatumKind::Plane { .. }) {
        return Err(ModelError::new(format!(
            "sketch datum '{id}' must be a plane"
        )));
    }
    Ok(Some(datum))
}

pub(crate) fn feature_signature(
    datums: &HashMap<&str, &DatumDefinition>,
    feature: &FeatureDefinition,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<Vec<u8>, ModelError> {
    let mut names = HashSet::new();
    collect_operation_parameters(&feature.operation, &mut names);
    let datum = sketch_datum(datums, &feature.operation)?;
    if let Some(DatumDefinition {
        kind: DatumKind::Plane { origin, normal },
        ..
    }) = datum
    {
        collect_vector_parameters(origin, &mut names);
        collect_vector_parameters(normal, &mut names);
    }
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort_unstable();
    let values = names
        .into_iter()
        .map(|name| {
            parameters
                .get(name)
                .map(|value| (name, value))
                .ok_or_else(|| ModelError::new(format!("unknown parameter '{name}'")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_vec(&FeatureSignature {
        feature,
        parameters: values,
        datum,
    })
    .map_err(|error| ModelError::new(format!("create feature signature: {error}")))
}

pub(crate) fn collect_operation_parameters<'a>(
    operation: &'a FeatureOperation,
    names: &mut HashSet<&'a str>,
) {
    match operation {
        FeatureOperation::Box { origin, size } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(size, names);
        }
        FeatureOperation::Cylinder {
            origin,
            axis,
            radius,
            height,
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(radius, names);
            collect_scalar_parameters(height, names);
        }
        FeatureOperation::SketchFace { sketch } | FeatureOperation::SketchWire { sketch } => {
            sketch.collect_parameters(names)
        }
        FeatureOperation::Translate { offset, .. } => collect_vector_parameters(offset, names),
        FeatureOperation::Extrude { direction, .. } => collect_vector_parameters(direction, names),
        FeatureOperation::Rib {
            thickness,
            direction,
            ..
        } => {
            collect_scalar_parameters(thickness, names);
            collect_vector_parameters(direction, names);
        }
        FeatureOperation::Hole {
            position,
            axis,
            diameter,
            extent,
            finish,
            thread,
            ..
        } => {
            collect_vector_parameters(position, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(diameter, names);
            if let Some(thread) = thread {
                collect_scalar_parameters(&thread.nominal_diameter, names);
                collect_scalar_parameters(&thread.pitch, names);
            }
            if let HoleExtent::Blind { depth } = extent {
                collect_scalar_parameters(depth, names);
            }
            match finish {
                HoleFinish::Plain => {}
                HoleFinish::Counterbore { diameter, depth } => {
                    collect_scalar_parameters(diameter, names);
                    collect_scalar_parameters(depth, names);
                }
                HoleFinish::Countersink {
                    diameter,
                    angle_radians,
                } => {
                    collect_scalar_parameters(diameter, names);
                    collect_scalar_parameters(angle_radians, names);
                }
            }
        }
        FeatureOperation::Rotate {
            origin,
            axis,
            angle_radians,
            ..
        }
        | FeatureOperation::Revolve {
            origin,
            axis,
            angle_radians,
            ..
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(angle_radians, names);
        }
        FeatureOperation::Fillet { edges, radius, .. } => {
            for selector in edges {
                collect_edge_selector_parameters(selector, names);
            }
            collect_scalar_parameters(radius, names);
        }
        FeatureOperation::Chamfer {
            edges, distance, ..
        } => {
            for selector in edges {
                collect_edge_selector_parameters(selector, names);
            }
            collect_scalar_parameters(distance, names);
        }
        FeatureOperation::Hollow {
            faces,
            thickness,
            tolerance,
            ..
        } => {
            for selector in faces {
                collect_face_selector_parameters(selector, names);
            }
            collect_scalar_parameters(thickness, names);
            collect_scalar_parameters(tolerance, names);
        }
        FeatureOperation::Sew { tolerance, .. } => {
            collect_scalar_parameters(tolerance, names);
        }
        FeatureOperation::Draft {
            faces,
            neutral_origin,
            neutral_normal,
            pull_direction,
            angle_radians,
            ..
        } => {
            for selector in faces {
                collect_face_selector_parameters(selector, names);
            }
            collect_vector_parameters(neutral_origin, names);
            collect_vector_parameters(neutral_normal, names);
            collect_vector_parameters(pull_direction, names);
            collect_scalar_parameters(angle_radians, names);
        }
        FeatureOperation::MakeSolid { .. } => {}
        FeatureOperation::Fuse { .. }
        | FeatureOperation::Cut { .. }
        | FeatureOperation::Common { .. } => {}
    }
}

pub(crate) fn collect_scalar_parameters<'a>(
    expression: &'a ScalarExpr,
    names: &mut HashSet<&'a str>,
) {
    match expression {
        ScalarExpr::Parameter(name) => {
            names.insert(name);
        }
        ScalarExpr::Negate(value)
        | ScalarExpr::Absolute(value)
        | ScalarExpr::Iso273ClearanceV1 {
            nominal_diameter: value,
            ..
        } => {
            collect_scalar_parameters(value, names);
        }
        ScalarExpr::Add(left, right)
        | ScalarExpr::Subtract(left, right)
        | ScalarExpr::Multiply(left, right)
        | ScalarExpr::Divide(left, right)
        | ScalarExpr::Minimum(left, right)
        | ScalarExpr::Maximum(left, right) => {
            collect_scalar_parameters(left, names);
            collect_scalar_parameters(right, names);
        }
        ScalarExpr::Clamp {
            value,
            minimum,
            maximum,
        } => {
            collect_scalar_parameters(value, names);
            collect_scalar_parameters(minimum, names);
            collect_scalar_parameters(maximum, names);
        }
        ScalarExpr::Conditional {
            left,
            right,
            when_true,
            when_false,
            ..
        } => {
            collect_scalar_parameters(left, names);
            collect_scalar_parameters(right, names);
            collect_scalar_parameters(when_true, names);
            collect_scalar_parameters(when_false, names);
        }
        ScalarExpr::Literal(_) => {}
    }
}

pub(crate) fn collect_vector_parameters<'a>(
    expression: &'a VectorExpr,
    names: &mut HashSet<&'a str>,
) {
    match expression {
        VectorExpr::Parameter(name) => {
            names.insert(name);
        }
        VectorExpr::Components { x, y, z } => {
            collect_scalar_parameters(x, names);
            collect_scalar_parameters(y, names);
            collect_scalar_parameters(z, names);
        }
        VectorExpr::Add(left, right) | VectorExpr::Subtract(left, right) => {
            collect_vector_parameters(left, names);
            collect_vector_parameters(right, names);
        }
        VectorExpr::Scale { vector, factor } => {
            collect_vector_parameters(vector, names);
            collect_scalar_parameters(factor, names);
        }
        VectorExpr::Normalize(vector) => collect_vector_parameters(vector, names),
        VectorExpr::Literal(_) => {}
    }
}

pub(crate) fn collect_edge_selector_parameters<'a>(
    selector: &'a EdgeSelector,
    names: &mut HashSet<&'a str>,
) {
    match selector {
        EdgeSelector::NearestCenter {
            target,
            maximum_distance,
        } => {
            collect_vector_parameters(target, names);
            collect_scalar_parameters(maximum_distance, names);
        }
        EdgeSelector::AtExtreme { tolerance, .. } => {
            collect_scalar_parameters(tolerance, names);
        }
        EdgeSelector::Longest {
            relative_tolerance, ..
        } => collect_scalar_parameters(relative_tolerance, names),
        EdgeSelector::CircularRadius { minimum, maximum }
        | EdgeSelector::CurvatureRadius { minimum, maximum }
        | EdgeSelector::CurvatureRadiusRange {
            minimum, maximum, ..
        } => {
            collect_scalar_parameters(minimum, names);
            collect_scalar_parameters(maximum, names);
        }
        EdgeSelector::CurvatureRadiusBounds {
            minimum,
            maximum,
            relative_tolerance,
            ..
        } => {
            collect_scalar_parameters(minimum, names);
            collect_scalar_parameters(maximum, names);
            collect_scalar_parameters(relative_tolerance, names);
        }
        EdgeSelector::Union(selectors) | EdgeSelector::Intersection(selectors) => {
            for selector in selectors {
                collect_edge_selector_parameters(selector, names);
            }
        }
        EdgeSelector::Difference { base, subtract } => {
            collect_edge_selector_parameters(base, names);
            collect_edge_selector_parameters(subtract, names);
        }
        EdgeSelector::History { source, .. } => collect_edge_selector_parameters(source, names),
    }
}

pub(crate) fn collect_face_selector_parameters<'a>(
    selector: &'a FaceSelector,
    names: &mut HashSet<&'a str>,
) {
    match selector {
        FaceSelector::NearestCenter {
            target,
            maximum_distance,
        } => {
            collect_vector_parameters(target, names);
            collect_scalar_parameters(maximum_distance, names);
        }
        FaceSelector::AtExtreme { tolerance, .. } => {
            collect_scalar_parameters(tolerance, names);
        }
        FaceSelector::NormalAligned {
            direction,
            minimum_dot,
        } => {
            collect_vector_parameters(direction, names);
            collect_scalar_parameters(minimum_dot, names);
        }
        FaceSelector::LargestArea {
            relative_tolerance, ..
        } => collect_scalar_parameters(relative_tolerance, names),
        FaceSelector::AdjacentToEdges { edges, .. } => {
            collect_edge_selector_parameters(edges, names);
        }
        FaceSelector::TangentTo { faces, .. } => {
            collect_face_selector_parameters(faces, names);
        }
        FaceSelector::Union(selectors) | FaceSelector::Intersection(selectors) => {
            for selector in selectors {
                collect_face_selector_parameters(selector, names);
            }
        }
        FaceSelector::Difference { base, subtract } => {
            collect_face_selector_parameters(base, names);
            collect_face_selector_parameters(subtract, names);
        }
        FaceSelector::History { source, .. } => collect_face_selector_parameters(source, names),
    }
}

pub(crate) fn validate_definition(definition: &FamilyDefinition) -> Result<(), ModelError> {
    if definition.id.is_empty() || definition.version == 0 {
        return Err(ModelError::new("family id and version are required"));
    }
    let mut feature_ids = HashSet::new();
    insert_unique_ids(
        &mut feature_ids,
        definition
            .features
            .iter()
            .map(|feature| feature.id.as_str()),
        "feature ids must be nonempty and unique",
    )?;
    let datums = definition
        .datums
        .iter()
        .map(|datum| (datum.id.as_str(), datum))
        .collect::<HashMap<_, _>>();
    for feature in &definition.features {
        match &feature.operation {
            FeatureOperation::SketchFace { sketch } | FeatureOperation::SketchWire { sketch } => {
                sketch
                    .validate_structure()
                    .map_err(|error| error.in_feature(&feature.id))?;
                sketch_datum(&datums, &feature.operation)
                    .map_err(|error| error.in_feature(&feature.id))?;
            }
            FeatureOperation::Sew { inputs, .. } if inputs.is_empty() => {
                return Err(ModelError::new(format!(
                    "feature '{}' requires at least one sewing input",
                    feature.id
                )));
            }
            FeatureOperation::MakeSolid { shells } if shells.is_empty() => {
                return Err(ModelError::new(format!(
                    "feature '{}' requires at least one shell input",
                    feature.id
                )));
            }
            _ => {}
        }
        if let Some(dependency) = feature
            .operation
            .dependencies()
            .into_iter()
            .find(|dependency| !feature_ids.contains(dependency))
        {
            return Err(ModelError::new(format!(
                "feature '{}' references unknown output '{dependency}'",
                feature.id
            )));
        }
    }
    let mut parameter_ids = HashSet::new();
    insert_unique_ids(
        &mut parameter_ids,
        definition
            .parameters
            .iter()
            .map(|parameter| parameter.id.as_str()),
        "parameter ids must be nonempty and unique",
    )?;
    insert_unique_ids(
        &mut parameter_ids,
        definition
            .derived_parameters
            .iter()
            .map(|parameter| parameter.id.as_str())
            .chain(
                definition
                    .derived_vector_parameters
                    .iter()
                    .map(|parameter| parameter.id.as_str()),
            ),
        "input and derived parameter ids must be nonempty and unique",
    )?;
    insert_unique_ids(
        &mut HashSet::new(),
        definition
            .constraints
            .iter()
            .map(|constraint| constraint.id.as_str()),
        "constraint ids must be nonempty and unique",
    )?;
    if definition
        .requirements
        .iter()
        .any(|requirement| requirement.version == 0)
    {
        return Err(ModelError::new(
            "requirement ids must be nonempty, versioned, and unique",
        ));
    }
    insert_unique_ids(
        &mut HashSet::new(),
        definition
            .requirements
            .iter()
            .map(|requirement| requirement.id.as_str()),
        "requirement ids must be nonempty, versioned, and unique",
    )?;
    assembly::validate_datums(definition)
}

/// Adds every id to `seen`, failing on the first empty or repeated id.
pub(crate) fn insert_unique_ids<'a>(
    seen: &mut HashSet<&'a str>,
    ids: impl IntoIterator<Item = &'a str>,
    message: &str,
) -> Result<(), ModelError> {
    for id in ids {
        if id.is_empty() || !seen.insert(id) {
            return Err(ModelError::new(message));
        }
    }
    Ok(())
}

pub(crate) fn cleanup_shapes<'session>(
    session: &Session,
    shapes: impl IntoIterator<Item = Shape<'session>>,
) {
    for shape in shapes {
        let _ = session.remove(shape);
    }
}

pub(crate) fn shape<'a, 'session>(
    shapes: &'a HashMap<String, Shape<'session>>,
    name: &str,
) -> Result<&'a Shape<'session>, ModelError> {
    shapes
        .get(name)
        .ok_or_else(|| ModelError::new(format!("named output '{name}' was not generated")))
}

pub(crate) fn verify_requirement(
    session: &Session,
    requirement: &Requirement,
    shapes: &HashMap<String, Shape<'_>>,
) -> Result<VerificationResult, ModelError> {
    let (passed, message) = match &requirement.rule {
        VerificationRule::ShapeValid { output } => {
            let passed = session.is_valid(shape(shapes, output)?)?;
            (
                passed,
                if passed {
                    "shape is valid"
                } else {
                    "shape is invalid"
                }
                .into(),
            )
        }
        VerificationRule::VolumeRange {
            output,
            minimum,
            maximum,
        } => {
            let volume = session.volume(shape(shapes, output)?)?;
            let minimum = minimum.cubic_millimeters()?;
            let maximum = maximum.cubic_millimeters()?;
            let passed = volume >= minimum && volume <= maximum;
            (
                passed,
                format!("volume {volume} mm^3; expected {minimum}..={maximum} mm^3"),
            )
        }
    };
    Ok(VerificationResult {
        requirement_id: requirement.id.clone(),
        status: if passed {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        message,
    })
}

pub(crate) fn instance_error(id: &str, error: ModelError) -> ModelError {
    error.context(&format!("instance '{id}'"))
}

pub(crate) fn release_results<'session>(
    session: &Session,
    results: impl IntoIterator<Item = GeneratedResult<'session>>,
) {
    for result in results {
        cleanup(session, result.shapes);
    }
}

/// Duplicates every named shape so another instance can own and place it.
pub(crate) fn duplicate_result<'session>(
    session: &'session Session,
    result: &GeneratedResult<'session>,
) -> Result<GeneratedResult<'session>, ModelError> {
    let mut shapes = HashMap::new();
    for (name, shape) in &result.shapes {
        match session.duplicate(shape) {
            Ok(copy) => {
                shapes.insert(name.clone(), copy);
            }
            Err(error) => {
                cleanup(session, shapes);
                return Err(error.into());
            }
        }
    }
    Ok(GeneratedResult {
        shapes,
        feature_signatures: result.feature_signatures.clone(),
        verification: result.verification.clone(),
        regeneration: result.regeneration.clone(),
    })
}

pub(crate) fn cleanup(session: &Session, shapes: HashMap<String, Shape<'_>>) {
    for shape in shapes.into_values() {
        let _ = session.remove(shape);
    }
}

pub(crate) fn apply_placement<'session>(
    session: &'session Session,
    result: GeneratedResult<'session>,
    placement: Placement,
) -> Result<GeneratedResult<'session>, ModelError> {
    let normalized = match placement.normalized() {
        Ok(normalized) => normalized,
        Err(error) => {
            cleanup(session, result.shapes);
            return Err(error);
        }
    };
    if normalized.rotation.is_none() && normalized.translation_is_zero() {
        return Ok(result);
    }

    let GeneratedResult {
        shapes,
        feature_signatures,
        verification,
        regeneration,
    } = result;
    let mut placed = HashMap::new();
    for (name, source) in &shapes {
        match place_shape(session, source, &normalized) {
            Ok(shape) => {
                placed.insert(name.clone(), shape);
            }
            Err(error) => {
                cleanup(session, placed);
                cleanup(session, shapes);
                return Err(error);
            }
        }
    }
    cleanup(session, shapes);
    Ok(GeneratedResult {
        shapes: placed,
        feature_signatures,
        verification,
        regeneration,
    })
}

/// Rotates then translates one shape for a non-identity placement, releasing
/// the intermediate rotated handle.
pub(crate) fn place_shape<'session>(
    session: &'session Session,
    source: &Shape<'session>,
    placement: &NormalizedPlacement,
) -> Result<Shape<'session>, ModelError> {
    let rotated = match placement.rotation {
        Some((origin, axis, angle)) => Some(session.rotate(source, origin, axis, angle)?),
        None => None,
    };
    if placement.translation_is_zero() {
        return Ok(rotated.expect("rotation exists for non-identity placement"));
    }
    let translated = session.translate(rotated.as_ref().unwrap_or(source), placement.translation);
    if let Some(intermediate) = rotated {
        let _ = session.remove(intermediate);
    }
    Ok(translated?)
}
