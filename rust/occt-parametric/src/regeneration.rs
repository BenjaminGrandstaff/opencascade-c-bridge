//! Feature-graph regeneration, incremental reuse, verification, and
//! result placement.

use super::*;

mod parameters;
mod requirements;
mod validation;

pub(crate) use parameters::*;
pub(crate) use requirements::*;
pub(crate) use validation::*;

pub struct GeneratedResult<'session> {
    pub(crate) shapes: HashMap<String, Shape<'session>>,
    pub(crate) feature_signatures: HashMap<String, Vec<u8>>,
    /// Colored faces per output, from the family's feature colors; face
    /// indices survive rigid placement.
    pub(crate) face_colors: HashMap<String, FaceColors>,
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

    /// Colored faces of an output as (face index in `Session::subshapes`
    /// order, linear RGB), ascending by index; empty when none are colored.
    pub fn face_colors(&self, name: &str) -> &[(usize, [f64; 3])] {
        self.face_colors.get(name).map_or(&[], Vec::as_slice)
    }

    pub fn named_outputs(&self) -> impl Iterator<Item = &str> {
        self.shapes.keys().map(String::as_str)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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

/// Geometry and check results for visualization only; never an accepted build.
pub struct DiagnosticGeneration<'session> {
    pub generated: GeneratedResult<'session>,
    pub verification_errors: Vec<(String, ModelError)>,
}

impl PartInstance<'_> {
    /// Generate a local diagnostic snapshot even when required checks fail.
    /// Geometry creation still validates results. Verification errors remain
    /// explicitly unverified; callers must not publish this as an accepted build.
    pub fn diagnostic_geometry<'session>(
        &self,
        session: &'session Session,
    ) -> Result<DiagnosticGeneration<'session>, ModelError> {
        validate_definition(self.definition)?;
        let parameters = self.resolved_parameters()?;
        let mut build = FeatureBuild::default();
        build.run(session, self.definition, &parameters, None)?;
        let mut verification = Vec::new();
        let mut errors = Vec::new();
        for requirement in &self.definition.requirements {
            match verify_requirement(session, requirement, &build.shapes) {
                Ok(result) => verification.push(result),
                Err(error) => errors.push((requirement.id.clone(), error)),
            }
        }
        Ok(DiagnosticGeneration {
            generated: GeneratedResult {
                shapes: build.shapes,
                feature_signatures: build.feature_signatures,
                face_colors: build.face_colors,
                verification,
                regeneration: build.regeneration,
            },
            verification_errors: errors,
        })
    }

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
                face_colors: build.face_colors,
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
    pub(crate) face_colors: HashMap<String, FaceColors>,
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
        let definitions = Features::new(definition);
        let mut pending: Vec<&FeatureDefinition> = definition.features.iter().collect();
        while !pending.is_empty() {
            let before = pending.len();
            let mut index = 0;
            while index < pending.len() {
                if self.is_ready(pending[index], &definitions) {
                    let feature = pending.remove(index);
                    self.add_feature(
                        session,
                        &datums,
                        &definitions,
                        feature,
                        parameters,
                        previous,
                    )?;
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

    pub(crate) fn is_ready(&self, feature: &FeatureDefinition, definitions: &Features<'_>) -> bool {
        dependencies_with_references(feature, &definitions.references)
            .iter()
            .all(|dependency| self.shapes.contains_key(*dependency))
    }

    /// Reuses the previous output through a duplicate handle when the feature
    /// signature is unchanged and no dependency was rebuilt; otherwise executes it.
    pub(crate) fn add_feature(
        &mut self,
        session: &'session Session,
        datums: &HashMap<&str, &DatumDefinition>,
        definitions: &Features<'_>,
        feature: &FeatureDefinition,
        parameters: &HashMap<String, ParameterValue>,
        previous: Option<&GeneratedResult<'session>>,
    ) -> Result<(), ModelError> {
        let signature = feature_signature(datums, feature, &definitions.references, parameters)?;
        let dependency_is_dirty = dependencies_with_references(feature, &definitions.references)
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
                execute_feature(
                    session,
                    datums,
                    definitions,
                    feature,
                    parameters,
                    &self.shapes,
                )
            }
        };
        let shape = generated.map_err(|error| error.in_feature(&feature.id))?;
        self.shapes.insert(feature.id.clone(), shape);
        // Recomputed even for reused outputs: duplicates keep their history,
        // so color edits need no geometry rebuild.
        if !definitions.colors.is_empty() {
            let colors = feature_face_colors(
                session,
                feature,
                definitions.colors.get(feature.id.as_str()).copied(),
                &self.shapes,
                &self.face_colors,
            )
            .map_err(|error| error.in_feature(&feature.id))?;
            if !colors.is_empty() {
                self.face_colors.insert(feature.id.clone(), colors);
            }
        }
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

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct FeatureSignature<'a> {
    pub(crate) feature: &'a FeatureDefinition,
    pub(crate) parameters: Vec<(&'a str, &'a ParameterValue)>,
    pub(crate) datum: Option<&'a DatumDefinition>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) references: Vec<&'a NamedReference>,
}

pub(crate) fn sketch_datum<'a>(
    datums: &HashMap<&str, &'a DatumDefinition>,
    operation: &FeatureOperation,
) -> Result<Option<&'a DatumDefinition>, ModelError> {
    let sketch = match operation {
        FeatureOperation::SketchFace { sketch }
        | FeatureOperation::SketchWire { sketch }
        | FeatureOperation::SketchOpenWire { sketch } => sketch,
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

/// The feature, the parameter values it reads, its sketch datum, and the
/// named references it uses: any change to these rebuilds it.
pub(crate) fn feature_signature(
    datums: &HashMap<&str, &DatumDefinition>,
    feature: &FeatureDefinition,
    references: &References<'_>,
    parameters: &HashMap<String, ParameterValue>,
) -> Result<Vec<u8>, ModelError> {
    let mut names = HashSet::new();
    collect_operation_parameters(&feature.operation, &mut names);
    let mut used = feature
        .operation
        .reference_names()
        .into_iter()
        .filter_map(|(name, _)| references.get(name).copied())
        .collect::<Vec<_>>();
    used.sort_by(|a, b| a.name.cmp(&b.name));
    used.dedup_by(|a, b| a.name == b.name);
    for reference in &used {
        match &reference.target {
            ReferenceTarget::Faces(selector) => {
                collect_face_selector_parameters(selector, &mut names)
            }
            ReferenceTarget::Edges(selector) => {
                collect_edge_selector_parameters(selector, &mut names)
            }
        }
    }
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
        references: used,
    })
    .map_err(|error| ModelError::new(format!("create feature signature: {error}")))
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
        face_colors: result.face_colors.clone(),
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
        face_colors,
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
        face_colors,
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
