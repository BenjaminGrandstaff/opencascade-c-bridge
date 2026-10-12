//! Guarded, family-wide sketch drafts for the live studio. Preview never mutates
//! the accepted document; apply repeats validation against the current version.
use super::*;
use occt_parametric::SketchDefinition;

fn sketch(operation: &Value) -> Option<&Value> {
    operation
        .as_object()?
        .values()
        .find_map(|value| value.get("sketch"))
}

impl Studio {
    pub(super) fn sketches(&self) -> Result<Value, String> {
        let features = self
            .document
            .family
            .features
            .iter()
            .filter_map(|feature| {
                let operation = serde_json::to_value(&feature.operation).ok()?;
                Some(json!({"feature": feature.id, "definition": sketch(&operation)?}))
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"version": self.version, "family": self.document.family.id, "sketches": features, "parameters": self.document.family.parameters}),
        )
    }

    /// Solve a temporary pin, then release it and verify the original sketch
    /// policy and all model requirements. Both passes are read-only previews.
    pub(super) fn sketch_drag(&mut self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Drag {
            version: u64,
            instance: String,
            feature: String,
            expected: SketchDefinition,
            definition: SketchDefinition,
            point: String,
            target: [f64; 2],
        }
        let edit: Drag = serde_json::from_slice(body).map_err(|e| format!("invalid drag: {e}"))?;
        if !edit.target.iter().all(|v| v.is_finite()) {
            return Err("Drag coordinates must be finite.".into());
        }
        let mut pinned = edit.definition.clone();
        let point = pinned
            .points
            .iter_mut()
            .find(|p| p.id == edit.point)
            .ok_or("Only authored points can be dragged.")?;
        if point.fixed {
            return Err(
                "This point is fixed. Edit its coordinates or explicitly unfix it first.".into(),
            );
        }
        use occt_parametric::{Quantity, ScalarExpr};
        if !matches!(point.x, ScalarExpr::Literal(_)) || !matches!(point.y, ScalarExpr::Literal(_))
        {
            return Err("This point has parameter or formula coordinates. Edit its driving controls instead.".into());
        }
        let coordinate =
            |value| ScalarExpr::Literal(Quantity::length(value, LengthUnit::Millimeter));
        point.x = coordinate(edit.target[0]);
        point.y = coordinate(edit.target[1]);
        point.fixed = true;
        let request = |definition: &SketchDefinition| json!({"version":edit.version,"instance":edit.instance,"feature":edit.feature,"expected":edit.expected,"definition":definition,"apply":false});
        let pinned_view =
            self.sketch_edit(&serde_json::to_vec(&request(&pinned)).map_err(|e| e.to_string())?)?;
        let scene = |view: &Value| -> Result<Value, String> {
            view["annotations"]["scenes"]
                .as_array()
                .and_then(|scenes| {
                    scenes
                        .iter()
                        .find(|s| s["kind"] == "sketch" && s["feature"] == edit.feature)
                })
                .cloned()
                .ok_or("Drag preview has no sketch scene.".into())
        };
        let pinned_scene = scene(&pinned_view)?;
        if pinned_scene["solver"]["solved"] != true {
            return Err(format!(
                "Drag of point '{}' did not converge with the existing constraints. Try a smaller move or edit a driving dimension.",
                edit.point
            ));
        }
        if pinned_view["can_apply"] != true {
            return Err(format!("Drag preview failed: {}", pinned_view["error"]));
        }
        let mut definition = edit.definition.clone();
        for point in &mut definition.points {
            if point.fixed {
                continue;
            }
            let solved = &pinned_scene["points"][&point.id];
            if let Some(x) = solved[0].as_f64()
                && matches!(point.x, ScalarExpr::Literal(_))
            {
                point.x = coordinate(x);
            }
            if let Some(y) = solved[1].as_f64()
                && matches!(point.y, ScalarExpr::Literal(_))
            {
                point.y = coordinate(y);
            }
        }
        let mut result = self
            .sketch_edit(&serde_json::to_vec(&request(&definition)).map_err(|e| e.to_string())?)?;
        if result["can_apply"] != true {
            return Err(format!("Released drag failed: {}", result["error"]));
        }
        let released = scene(&result)?;
        let solved = &released["points"][&edit.point];
        let target_met = edit.target.iter().enumerate().all(|(axis, target)| {
            solved[axis].as_f64().is_some_and(|value| {
                (value - target).abs() <= (1e-6 + 32.0 * f64::EPSILON * target.abs())
            })
        });
        if !target_met {
            return Err("The released solve moved the point away from the target. Use a driving dimension to define this position.".into());
        }
        result["definition"] = json!(definition);
        result["dragged_point"] = json!(edit.point);
        Ok(result)
    }

    pub(super) fn sketch_edit(&mut self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Edit {
            version: u64,
            instance: String,
            feature: String,
            expected: SketchDefinition,
            definition: SketchDefinition,
            apply: bool,
        }
        let edit: Edit =
            serde_json::from_slice(body).map_err(|e| format!("invalid sketch edit: {e}"))?;
        if edit.version != self.version {
            return Err("The model changed. Reload the sketch before applying this draft.".into());
        }
        if !primary_instances(&self.document)?.contains(&edit.instance) {
            return Err("unknown editable instance".into());
        }
        let mut document = self.document.clone();
        let feature = document
            .family
            .features
            .iter_mut()
            .find(|f| f.id == edit.feature)
            .ok_or("unknown sketch feature")?;
        let mut operation = serde_json::to_value(&feature.operation).map_err(|e| e.to_string())?;
        let original = sketch(&operation).ok_or("feature has no authored sketch")?;
        let expected = serde_json::to_value(&edit.expected).map_err(|e| e.to_string())?;
        if original != &expected {
            return Err("Sketch definition changed. Reload before applying this draft.".into());
        }
        if edit.expected.id != edit.definition.id {
            return Err("sketch identity must stay the same".into());
        }
        if edit.expected == edit.definition {
            return Err("The sketch draft has no changes.".into());
        }
        for value in operation
            .as_object_mut()
            .ok_or("invalid feature operation")?
            .values_mut()
        {
            if let Some(target) = value.get_mut("sketch") {
                *target = serde_json::to_value(&edit.definition).map_err(|e| e.to_string())?;
            }
        }
        feature.operation = serde_json::from_value(operation).map_err(|e| e.to_string())?;
        document.family.version = document
            .family
            .version
            .checked_add(1)
            .ok_or("family version overflow")?;
        let document =
            ModelDocument::from_json(&document.to_json_pretty().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let output = self
            .output
            .clone()
            .or_else(|| document.family.features.last().map(|f| f.id.clone()))
            .ok_or("no output feature")?;
        let outputs = [occt_parametric::InstanceOutputRef {
            instance: edit.instance,
            output: output.clone(),
        }];
        let mut annotations = view_data::collect(
            &document,
            &outputs,
            true,
            &view_data::ViewOptions::default(),
        )
        .map_err(|e| e.to_string())?;
        annotations["version"] = json!(self.version);
        annotations["accepted"] = json!(false);
        annotations["preview"] = json!(true);
        let generated = build(&document, self.output.as_deref()).and_then(|(gltf, report)| {
            let results = report["variants"]
                .as_object()
                .into_iter()
                .flat_map(|variants| variants.values())
                .flat_map(|results| results.as_array().into_iter().flatten())
                .chain(report["assembly"].as_array().into_iter().flatten());
            for result in results {
                let id = result["requirement"].as_str().unwrap_or("");
                if report["requirements"][id]["priority"] == "required" && result["passed"] != true
                {
                    return Err(format!(
                        "Required check '{id}' failed: {}",
                        result["message"]
                    ));
                }
            }
            Ok((gltf, report))
        });
        let rejection = generated.as_ref().err().cloned();
        if edit.apply {
            let (gltf, requirements) = generated?;
            self.accept(document, gltf, requirements, self.output.clone())?;
            Ok(json!({"applied": true, "state": self.state()}))
        } else {
            Ok(
                json!({"applied": false, "can_apply": rejection.is_none(), "error": rejection, "annotations": annotations}),
            )
        }
    }
}
