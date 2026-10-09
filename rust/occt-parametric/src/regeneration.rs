//! Feature-graph regeneration, incremental reuse, verification, and
//! result placement.

use super::*;

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

pub(crate) fn collect_operation_parameters<'a>(
    operation: &'a FeatureOperation,
    names: &mut HashSet<&'a str>,
) {
    match operation {
        FeatureOperation::SheetMetal { definition } => definition.collect_parameters(names),
        FeatureOperation::SheetMetalFlat { neutral_factor, .. } => {
            collect_scalar_parameters(neutral_factor, names)
        }
        FeatureOperation::Box { origin, size } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(size, names);
        }
        FeatureOperation::Loft { sections, .. } => collect_loft_parameters(sections, names),
        FeatureOperation::ProfileLoft { .. } | FeatureOperation::PlanarRegion { .. } => {}
        FeatureOperation::Sweep { orientation, .. } => {
            if let SweepOrientation::Binormal { direction } = orientation {
                collect_vector_parameters(direction, names);
            }
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
        FeatureOperation::Cone {
            origin,
            axis,
            base_radius,
            top_radius,
            height,
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(base_radius, names);
            collect_scalar_parameters(top_radius, names);
            collect_scalar_parameters(height, names);
        }
        FeatureOperation::Sphere { center, radius } => {
            collect_vector_parameters(center, names);
            collect_scalar_parameters(radius, names);
        }
        FeatureOperation::SketchFace { sketch }
        | FeatureOperation::SketchWire { sketch }
        | FeatureOperation::SketchOpenWire { sketch } => sketch.collect_parameters(names),
        FeatureOperation::Translate { offset, .. } => collect_vector_parameters(offset, names),
        FeatureOperation::Extrude {
            direction, extent, ..
        } => {
            collect_vector_parameters(direction, names);
            if let ExtrudeExtent::UpToFace { face, .. } = extent {
                collect_face_selector_parameters(face, names);
            }
        }
        FeatureOperation::Rib {
            thickness,
            direction,
            profile_mode,
            ..
        } => {
            collect_scalar_parameters(thickness, names);
            collect_vector_parameters(direction, names);
            match profile_mode {
                RibProfileMode::Closed => {}
                RibProfileMode::OpenStrip { offset } => collect_vector_parameters(offset, names),
                RibProfileMode::OpenToNext {
                    direction,
                    maximum_length,
                } => {
                    collect_vector_parameters(direction, names);
                    collect_scalar_parameters(maximum_length, names);
                }
            }
        }
        FeatureOperation::Hole {
            position,
            axis,
            diameter,
            extent,
            bottom,
            finish,
            thread,
            ..
        } => {
            collect_vector_parameters(position, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(diameter, names);
            collect_hole_parameters(extent, bottom, finish, thread.as_deref(), names);
        }

        FeatureOperation::Thread {
            origin,
            axis,
            major_diameter,
            pitch,
            length,
            ..
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_scalar_parameters(major_diameter, names);
            collect_scalar_parameters(pitch, names);
            collect_scalar_parameters(length, names);
        }
        FeatureOperation::Helix {
            origin,
            axis,
            start,
            radius,
            pitch,
            turns,
            ..
        } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(axis, names);
            collect_vector_parameters(start, names);
            collect_scalar_parameters(radius, names);
            collect_scalar_parameters(pitch, names);
            collect_scalar_parameters(turns, names);
        }
        FeatureOperation::Mirror { origin, normal, .. } => {
            collect_vector_parameters(origin, names);
            collect_vector_parameters(normal, names);
        }
        FeatureOperation::Scale { center, factor, .. } => {
            collect_vector_parameters(center, names);
            collect_scalar_parameters(factor, names);
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
        FeatureOperation::VariableFillet {
            edges,
            start_radius,
            end_radius,
            stations,
            spine_direction,
            ..
        } => {
            for selector in edges {
                collect_edge_selector_parameters(selector, names);
            }
            collect_scalar_parameters(start_radius, names);
            collect_scalar_parameters(end_radius, names);
            collect_fillet_law_parameters(stations, spine_direction, names);
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
        FeatureOperation::Offset {
            distance,
            tolerance,
            ..
        } => {
            collect_scalar_parameters(distance, names);
            collect_scalar_parameters(tolerance, names);
        }
        FeatureOperation::Unify {
            linear_tolerance,
            angular_tolerance,
            ..
        } => {
            collect_scalar_parameters(linear_tolerance, names);
            collect_scalar_parameters(angular_tolerance, names);
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

fn collect_fillet_law_parameters<'a>(
    stations: &'a [FilletRadiusStation],
    direction: &'a FilletSpineDirection,
    names: &mut HashSet<&'a str>,
) {
    for station in stations {
        collect_scalar_parameters(&station.position, names);
        collect_scalar_parameters(&station.radius, names);
    }
    if let FilletSpineDirection::FromPoint { point } = direction {
        collect_vector_parameters(point, names);
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
        ScalarExpr::CarrLaneTapDrillV1 {
            nominal_diameter,
            pitch,
            ..
        } => {
            collect_scalar_parameters(nominal_diameter, names);
            collect_scalar_parameters(pitch, names);
        }
        ScalarExpr::Negate(value)
        | ScalarExpr::Absolute(value)
        | ScalarExpr::SquareRoot(value)
        | ScalarExpr::Sine(value)
        | ScalarExpr::Cosine(value)
        | ScalarExpr::Tangent(value)
        | ScalarExpr::ArcSine(value)
        | ScalarExpr::ArcCosine(value)
        | ScalarExpr::CarrLaneSocketHeadV1 {
            nominal_diameter: value,
            ..
        }
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
        | ScalarExpr::Maximum(left, right)
        | ScalarExpr::Hypotenuse(left, right)
        | ScalarExpr::Power {
            base: left,
            exponent: right,
        }
        | ScalarExpr::ArcTangent2 { y: left, x: right }
        | ScalarExpr::RoundToStep {
            value: left,
            step: right,
            ..
        } => {
            collect_scalar_parameters(left, names);
            collect_scalar_parameters(right, names);
        }
        ScalarExpr::Interpolate { from, to, fraction } => {
            collect_scalar_parameters(from, names);
            collect_scalar_parameters(to, names);
            collect_scalar_parameters(fraction, names);
        }
        ScalarExpr::VectorLength(value) => collect_vector_parameters(value, names),
        ScalarExpr::DotProduct(left, right) => {
            collect_vector_parameters(left, names);
            collect_vector_parameters(right, names);
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
        EdgeSelector::Persistent { select, .. } => collect_edge_selector_parameters(select, names),
        // A named reference's parameters join its users' signatures directly.
        EdgeSelector::Named(_) => {}
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
        FaceSelector::TangentTo {
            faces,
            angular_tolerance,
            ..
        } => {
            collect_face_selector_parameters(faces, names);
            if let Some(tolerance) = angular_tolerance {
                collect_scalar_parameters(tolerance, names);
            }
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
        FaceSelector::Persistent { select, .. } => collect_face_selector_parameters(select, names),
        FaceSelector::Named(_) => {}
        FaceSelector::GeneratedFromEdges { source, .. } => {
            collect_edge_selector_parameters(source, names)
        }
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
    let references = validate_references(definition, &feature_ids)?;
    for (feature, color) in &definition.feature_colors {
        if !feature_ids.contains(feature.as_str()) {
            return Err(ModelError::new(format!(
                "feature color names unknown feature '{feature}'"
            )));
        }
        if !color
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
        {
            return Err(ModelError::new(format!(
                "feature '{feature}' color channels must be in [0, 1]"
            )));
        }
    }
    let datums = definition
        .datums
        .iter()
        .map(|datum| (datum.id.as_str(), datum))
        .collect::<HashMap<_, _>>();
    for feature in &definition.features {
        for (name, kind) in feature.operation.reference_names() {
            let found = references.get(name).ok_or_else(|| {
                ModelError::new(format!(
                    "feature '{}' uses unknown named reference '{name}'",
                    feature.id
                ))
            })?;
            if found.target.kind() != kind {
                return Err(ModelError::new(format!(
                    "feature '{}' uses named reference '{name}' as the wrong kind of topology",
                    feature.id
                )));
            }
        }
        match &feature.operation {
            FeatureOperation::SketchFace { sketch }
            | FeatureOperation::SketchWire { sketch }
            | FeatureOperation::SketchOpenWire { sketch } => {
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
    validate_traces(definition, &feature_ids, &parameter_ids)?;
    assembly::validate_datums(definition)
}

/// Checks named references: unique nonempty names, targets that use no other
/// names, and features that exist. O(references + their selectors).
fn validate_references<'a>(
    definition: &'a FamilyDefinition,
    feature_ids: &HashSet<&str>,
) -> Result<References<'a>, ModelError> {
    insert_unique_ids(
        &mut HashSet::new(),
        definition
            .references
            .iter()
            .map(|reference| reference.name.as_str()),
        "named reference names must be nonempty and unique",
    )?;
    for reference in &definition.references {
        let mut names = Vec::new();
        reference.target.names(&mut names);
        if !names.is_empty() {
            return Err(ModelError::new(format!(
                "named reference '{}' cannot use other named references",
                reference.name
            )));
        }
        let mut dependencies = Vec::new();
        reference.target.dependencies(&mut dependencies);
        if let Some(missing) = dependencies
            .into_iter()
            .find(|dependency| !feature_ids.contains(dependency))
        {
            return Err(ModelError::new(format!(
                "named reference '{}' references unknown output '{missing}'",
                reference.name
            )));
        }
    }
    Ok(reference_map(definition))
}

/// Assumptions have unique ids and statements; every requirement trace names
/// an existing feature, parameter, or assumption. O(assumptions + traces).
fn validate_traces(
    definition: &FamilyDefinition,
    feature_ids: &HashSet<&str>,
    parameter_ids: &HashSet<&str>,
) -> Result<(), ModelError> {
    let mut assumptions = HashSet::new();
    insert_unique_ids(
        &mut assumptions,
        definition
            .assumptions
            .iter()
            .map(|assumption| assumption.id.as_str()),
        "assumption ids must be nonempty and unique",
    )?;
    if let Some(assumption) = definition
        .assumptions
        .iter()
        .find(|assumption| assumption.statement.trim().is_empty())
    {
        return Err(ModelError::new(format!(
            "assumption '{}' needs a statement",
            assumption.id
        )));
    }
    for requirement in &definition.requirements {
        for trace in &requirement.traces {
            let (known, kind, id) = match trace {
                TraceTarget::Feature(id) => (feature_ids.contains(id.as_str()), "feature", id),
                TraceTarget::Parameter(id) => {
                    (parameter_ids.contains(id.as_str()), "parameter", id)
                }
                TraceTarget::Assumption(id) => {
                    (assumptions.contains(id.as_str()), "assumption", id)
                }
            };
            if !known {
                return Err(ModelError::new(format!(
                    "requirement '{}' traces to unknown {kind} '{id}'",
                    requirement.id
                )));
            }
        }
    }
    Ok(())
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
    let id = requirement.id.as_str();
    Ok(match &requirement.rule {
        VerificationRule::ShapeValid { output } => {
            let passed = session.is_valid(shape(shapes, output)?)?;
            let message = if passed {
                "shape is valid"
            } else {
                "shape is invalid"
            };
            VerificationResult::exact(id, passed, message.into())
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
            VerificationResult::exact(
                id,
                passed,
                format!("volume {volume} mm^3; expected {minimum}..={maximum} mm^3"),
            )
            .measured(Measurement {
                value: volume,
                unit: MeasurementUnit::CubicMillimeter,
                minimum: Some(minimum),
                maximum: Some(maximum),
            })
        }
        VerificationRule::Connectivity {
            output,
            solids,
            allow_voids,
        } => {
            if *solids == 0 {
                return Err(ModelError::new("connectivity requires at least one solid"));
            }
            let found = connectivity(session, shape(shapes, output)?)?;
            let loose = found.loose_faces + found.loose_edges + found.loose_vertices;
            let voids_ok = *allow_voids || found.maximum_shells_per_solid <= 1;
            let passed = found.solids == *solids as usize && loose == 0 && voids_ok;
            VerificationResult::exact(
                id,
                passed,
                format!(
                    "{} solid(s), expected {solids}; up to {} shell(s) per solid{}; \
                     {} loose face(s), {} loose edge(s), {} loose vertex(es)",
                    found.solids,
                    found.maximum_shells_per_solid,
                    if *allow_voids { " (voids allowed)" } else { "" },
                    found.loose_faces,
                    found.loose_edges,
                    found.loose_vertices,
                ),
            )
            .measured(Measurement {
                value: found.solids as f64,
                unit: MeasurementUnit::Count,
                minimum: Some(f64::from(*solids)),
                maximum: Some(f64::from(*solids)),
            })
        }
        VerificationRule::MinimumRadius {
            output,
            minimum,
            side,
            sharp_edges,
            samples_per_direction,
        } => minimum_radius(
            session,
            id,
            shape(shapes, output)?,
            *minimum,
            *side,
            *sharp_edges,
            *samples_per_direction,
        )?,
        VerificationRule::FitsWithin { output, envelope } => {
            fits_within(session, id, shape(shapes, output)?, *envelope)?
        }
        VerificationRule::MinimumWall {
            output,
            minimum,
            mesh,
            maximum_samples,
        } => screen(
            session,
            id,
            shape(shapes, output)?,
            *mesh,
            Screen::Wall {
                minimum: *minimum,
                maximum_samples: *maximum_samples,
            },
        )?,
        VerificationRule::DraftAngle {
            output,
            pull_direction,
            minimum_radians,
            mesh,
        } => screen(
            session,
            id,
            shape(shapes, output)?,
            *mesh,
            Screen::Draft {
                pull_direction: *pull_direction,
                minimum_radians: *minimum_radians,
            },
        )?,
        VerificationRule::Undercut {
            output,
            pull_direction,
            parting_origin,
            tolerance_radians,
            mesh,
        } => undercut(
            session,
            id,
            shape(shapes, output)?,
            *mesh,
            *pull_direction,
            *parting_origin,
            *tolerance_radians,
        )?,
        VerificationRule::Overhang {
            output,
            build_direction,
            maximum_radians,
            mesh,
        } => screen(
            session,
            id,
            shape(shapes, output)?,
            *mesh,
            Screen::Overhang {
                build_direction: *build_direction,
                maximum_radians: *maximum_radians,
            },
        )?,
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

fn collect_hole_parameters<'a>(
    extent: &'a HoleExtent,
    bottom: &'a HoleBottom,
    finish: &'a HoleFinish,
    thread: Option<&'a ThreadSpecification>,
    names: &mut HashSet<&'a str>,
) {
    if let HoleBottom::DrillPoint { angle_radians } = bottom {
        collect_scalar_parameters(angle_radians, names);
    }
    if let Some(thread) = thread {
        collect_scalar_parameters(&thread.nominal_diameter, names);
        collect_scalar_parameters(&thread.pitch, names);
    }
    match extent {
        HoleExtent::Blind { depth } => collect_scalar_parameters(depth, names),
        HoleExtent::UpToFace { face } => collect_face_selector_parameters(face, names),
        _ => {}
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
