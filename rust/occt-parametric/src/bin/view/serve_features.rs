//! Guarded sketch-to-solid feature authoring. Drafts contain append or replace
//! operations, never requirement removal or changes to the accepted document.
use super::*;
use occt_parametric::{
    FeatureDefinition, FeatureOperation, Requirement, RequirementKind, RequirementPriority,
    VerificationRule, Volume,
};

impl Studio {
    pub(super) fn feature_view(&self) -> Value {
        let dependencies = self.document.family.feature_inputs().ok();
        json!({"version":self.version,"family":self.document.family.id,
            "history":self.document.family.history,"feature_states":self.document.family.history_states().ok().map(|states|states.into_iter().map(|(id,state)|{let mut value=serde_json::to_value(state).unwrap();value["feature"]=json!(id);value}).collect::<Vec<_>>()),
            "features":self.document.family.features,"parameters":self.document.family.parameters,"dependencies":dependencies,
            "output":self.output.as_deref().or_else(||self.document.family.history_output().ok().flatten()),
            "thread_sizes":thread_sizes()})
    }
    pub(super) fn feature_edit(&mut self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Action {
            Add,
            Replace,
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Edit {
            version: u64,
            instance: String,
            expected: Vec<FeatureDefinition>,
            action: Action,
            features: Vec<FeatureDefinition>,
            output: String,
            apply: bool,
        }
        let edit: Edit =
            serde_json::from_slice(body).map_err(|e| format!("invalid feature edit: {e}"))?;
        if edit.version != self.version || edit.expected != self.document.family.features {
            return Err(
                "The feature graph changed. Reload the solid controls before applying this draft."
                    .into(),
            );
        }
        if !primary_instances(&self.document)?.contains(&edit.instance) {
            return Err("Unknown editable instance.".into());
        }
        if edit.features.is_empty() || edit.features.len() > 3 {
            return Err("A solid edit must contain one to three features.".into());
        }
        if matches!(edit.action, Action::Add)
            && edit.features.last().is_none_or(|f| f.id != edit.output)
        {
            return Err("The new result output must be the final added feature.".into());
        }
        let mut document = self.document.clone();
        let mut ids = HashSet::new();
        for feature in &edit.features {
            if !ids.insert(&feature.id) {
                return Err("Feature IDs must be distinct.".into());
            }
            match &feature.operation {
                FeatureOperation::Hole { .. }
                | FeatureOperation::Fillet { .. }
                | FeatureOperation::Chamfer { .. }
                | FeatureOperation::Extrude { .. }
                | FeatureOperation::ProfileLoft { .. }
                | FeatureOperation::Sweep { .. }
                | FeatureOperation::Revolve { .. }
                | FeatureOperation::Fuse { .. }
                | FeatureOperation::Cut { .. }
                | FeatureOperation::Hollow { .. }
                | FeatureOperation::Draft { .. }
                | FeatureOperation::Mirror { .. }
                | FeatureOperation::LinearPattern { .. }
                | FeatureOperation::CircularPattern { .. }
                | FeatureOperation::SheetMetal { .. }
                | FeatureOperation::SheetMetalFlat { .. } => (),
                _ => {
                    return Err(
                        "Solid controls support extrusions, revolutions, sweeps, profile lofts, fillets, chamfers, holes, shells, drafts, mirrors, patterns, sheet metal, unions and cuts."
                            .into(),
                    );
                }
            }
            match edit.action {
                Action::Add => {
                    if document.family.features.iter().any(|f| f.id == feature.id) {
                        return Err(format!("Feature '{}' already exists.", feature.id));
                    }
                    document.family.features.push(feature.clone());
                }
                Action::Replace => {
                    let existing = document
                        .family
                        .features
                        .iter_mut()
                        .find(|f| f.id == feature.id)
                        .ok_or("Unknown feature to replace.")?;
                    if std::mem::discriminant(&existing.operation)
                        != std::mem::discriminant(&feature.operation)
                    {
                        return Err("An edited feature must retain its operation type.".into());
                    }
                    *existing = feature.clone();
                }
            }
        }
        if document.family.features == self.document.family.features {
            return Err("The solid draft has no changes.".into());
        }
        // Persist the basic result guarantees so future parameter edits and
        // CLI/MCP regeneration enforce them, rather than just this preview.
        let guarantees = [
            (
                "valid",
                VerificationRule::ShapeValid {
                    output: edit.output.clone(),
                },
                "must remain valid",
            ),
            (
                "material",
                VerificationRule::VolumeRange {
                    output: edit.output.clone(),
                    minimum: Volume {
                        value: f64::from_bits(1),
                        unit: LengthUnit::Millimeter,
                    },
                    maximum: Volume {
                        value: f64::MAX,
                        unit: LengthUnit::Millimeter,
                    },
                },
                "must retain positive solid volume",
            ),
        ];
        let mut taken: HashSet<String> = std::iter::once(&document.family)
            .chain(&document.additional_families)
            .flat_map(|f| f.requirements.iter().map(|r| r.id.clone()))
            .chain(document.assembly.requirements.iter().map(|r| r.id.clone()))
            .collect();
        for (suffix, rule, statement) in guarantees {
            if document
                .family
                .requirements
                .iter()
                .any(|r| r.priority == RequirementPriority::Required && r.rule == rule)
            {
                continue;
            }
            let prefix = format!("{}-solid-{suffix}", edit.output);
            let id = (0..)
                .map(|n| {
                    if n == 0 {
                        prefix.clone()
                    } else {
                        format!("{prefix}-{n}")
                    }
                })
                .find(|id| taken.insert(id.clone()))
                .expect("unused requirement name");
            document.family.requirements.push(Requirement {
                id,
                version: 1,
                kind: RequirementKind::Validation,
                priority: RequirementPriority::Required,
                statement: format!("{} {statement}", edit.output),
                rule,
                provenance: "occt-view solid authoring".into(),
                traces: vec![],
            });
        }
        document.family.version = document
            .family
            .version
            .checked_add(1)
            .ok_or("Family version overflow.")?;
        let document =
            ModelDocument::from_json(&document.to_json_pretty().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if !document.family.features.iter().any(|f| f.id == edit.output) {
            return Err("Unknown result output.".into());
        }
        let outputs = [occt_parametric::InstanceOutputRef {
            instance: edit.instance.clone(),
            output: edit.output.clone(),
        }];
        let annotations = if edit.apply {
            None
        } else {
            view_data::collect(
                &document,
                &outputs,
                true,
                &view_data::ViewOptions::default(),
            )
            .map(|mut data| {
                data["version"] = json!(self.version);
                data["accepted"] = json!(false);
                data["preview"] = json!(true);
                data
            })
            .ok()
        };
        let generated = build_solid(&document, &edit.output, &edit.features);
        if edit.apply {
            let (gltf, requirements, geometry) = generated?;
            self.accept(document, gltf, requirements, Some(edit.output.clone()))?;
            Ok(
                json!({"applied":true,"output":edit.output,"geometry":geometry,"state":self.state()}),
            )
        } else {
            match generated {
                Ok((gltf, _, geometry)) => {
                    let gltf: Value = serde_json::from_str(&gltf).map_err(|e| e.to_string())?;
                    Ok(
                        json!({"applied":false,"can_apply":true,"version":self.version,"instance":edit.instance,"output":edit.output,"geometry":geometry,"annotations":annotations,"gltf":gltf}),
                    )
                }
                Err(error) => Ok(
                    json!({"applied":false,"can_apply":false,"version":self.version,"instance":edit.instance,"output":edit.output,"error":error,"annotations":annotations}),
                ),
            }
        }
    }
}
fn build_solid(
    document: &ModelDocument,
    output: &str,
    changed: &[FeatureDefinition],
) -> Result<(String, Value, Value), String> {
    let primary = primary_instances(document)?;
    let mut graph = document.instance_graph().map_err(|e| e.to_string())?;
    let session = Session::new().map_err(|e| e.to_string())?;
    let generation = graph.regenerate_all(&session).map_err(|e| e.to_string())?;
    let mut geometry = serde_json::Map::new();
    let mut checked: HashMap<&str, Value> = HashMap::new();
    for (id, result) in generation
        .instances()
        .filter(|(id, _)| primary.contains(*id))
    {
        let variant = generation.shared_from(id).unwrap_or(id);
        if let Some(evidence) = checked.get(variant) {
            geometry.insert(id.to_owned(), evidence.clone());
            continue;
        }
        let shape = result
            .shape(output)
            .ok_or("Result output is unavailable.")?;
        let solids = session
            .subshape_count(shape, occt_bridge::ShapeType::Solid)
            .map_err(|e| e.to_string())?;
        let volume = session.volume(shape).map_err(|e| e.to_string())?;
        if solids == 0
            || !volume.is_finite()
            || volume <= 0.0
            || !session.is_valid(shape).map_err(|e| e.to_string())?
        {
            return Err(format!(
                "Result '{output}' on '{id}' must be a valid, nonempty solid."
            ));
        }
        for feature in changed {
            let (target, cut) = match &feature.operation {
                FeatureOperation::Cut { object, .. } => (object, true),
                FeatureOperation::Fuse { left, .. } => (left, false),
                _ => continue,
            };
            let target_shape = result
                .shape(target)
                .ok_or("Boolean target is unavailable.")?;
            if session
                .subshape_count(target_shape, occt_bridge::ShapeType::Solid)
                .map_err(|e| e.to_string())?
                == 0
            {
                return Err("Boolean target must contain solid material.".into());
            }
            let before = session.volume(target_shape).map_err(|e| e.to_string())?;
            let after = session
                .volume(
                    result
                        .shape(&feature.id)
                        .ok_or("Boolean result is unavailable.")?,
                )
                .map_err(|e| e.to_string())?;
            let tolerance = 1e-9 + 64.0 * f64::EPSILON * before.abs();
            if !before.is_finite()
                || !after.is_finite()
                || (if cut { before - after } else { after - before }) <= tolerance
            {
                return Err(format!(
                    "Feature '{}' makes no material {} on '{id}'.",
                    feature.id,
                    if cut { "removal" } else { "addition" }
                ));
            }
        }
        let evidence = json!({"valid":true,"solids":solids,"volume_mm3":volume});
        geometry.insert(id.to_owned(), evidence.clone());
        checked.insert(variant, evidence);
    }
    if geometry.is_empty() {
        return Err("No primary-family solid was generated.".into());
    }
    let gltf = graph
        .export_gltf_generated(&session, &generation, output, MeshSettings::default())
        .map_err(|e| e.to_string())?;
    Ok((
        gltf,
        requirement_report(document, &generation),
        Value::Object(geometry),
    ))
}

/// The tap sizes the studio offers for threaded holes: the frozen Carr Lane
/// V1 catalog, whose tap-drill expression the hole diameter can follow.
/// Inch sizes name their Unified (B1.1) series and standard 2B designation
/// where the core's size table has one. O(sizes).
fn thread_sizes() -> Value {
    use occt_parametric::{
        HoleCatalogSystem, UnifiedClass, UnifiedSeries, UnifiedThread, carr_lane_tap_sizes_v1,
    };
    let trim = |value: f64| {
        let text = format!("{value:.4}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    let metric: Vec<Value> = carr_lane_tap_sizes_v1(HoleCatalogSystem::Metric)
        .into_iter()
        .map(|size| {
            json!({"label": format!("M{}x{}", trim(size.nominal_mm), trim(size.pitch_mm)),
                "nominal": {"value": size.nominal, "dimension": "length", "unit": "millimeter"},
                "pitch": {"value": size.pitch_mm, "dimension": "length", "unit": "millimeter"},
                "drill_mm": size.drill_mm})
        })
        .collect();
    let inch: Vec<Value> = carr_lane_tap_sizes_v1(HoleCatalogSystem::Inch)
        .into_iter()
        .map(|size| {
            let unified = [UnifiedSeries::Unc, UnifiedSeries::Unf, UnifiedSeries::Unef]
                .into_iter()
                .find_map(|series| {
                    UnifiedThread::new(size.nominal_mm, size.pitch_mm, series, UnifiedClass::TwoB, false)
                        .ok()
                        .map(|thread| (series, thread.designation()))
                });
            let label = match &unified {
                Some((_, designation)) => designation.split(' ').next().unwrap_or_default().to_owned(),
                None => format!("{}-{}", trim(size.nominal), trim(size.threads_per_inch)),
            };
            json!({"label": label,
                "series": unified.as_ref().map(|(series, _)| series),
                "designation": unified.as_ref().map(|(_, designation)| designation),
                "nominal": {"value": size.nominal, "dimension": "length", "unit": "inch"},
                "pitch": {"value": 1.0 / size.threads_per_inch, "dimension": "length", "unit": "inch"},
                "drill_mm": size.drill_mm})
        })
        .collect();
    json!({"metric": metric, "inch": inch})
}
