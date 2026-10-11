//! Focused read-only sketch diagnosis. Unrelated/downstream failures do not
//! prevent inspecting a source sketch; dependency geometry is built in isolation.
use super::*;

fn subset(
    family: &FamilyDefinition,
    feature: &str,
    include: bool,
) -> Result<FamilyDefinition, Failure> {
    let inputs = family
        .feature_inputs()
        .map_err(|e| model_failure("validation", e))?;
    let mut ids = HashSet::new();
    let mut stack = vec![feature];
    while let Some(id) = stack.pop() {
        if ids.insert(id) {
            stack.extend(inputs[id].iter().copied());
        }
    }
    let mut f = family.clone();
    f.features
        .retain(|item| ids.contains(item.id.as_str()) && (include || item.id != feature));
    let names = f
        .features
        .iter()
        .flat_map(|item| item.operation.named_reference_names())
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    f.references.retain(|r| names.contains(&r.name));
    f.feature_colors
        .retain(|id, _| f.features.iter().any(|item| &item.id == id));
    f.requirements.clear();
    Ok(f)
}

pub(super) fn inspect(
    part: &PartInstance<'_>,
    feature: &str,
    offset: usize,
    limit: usize,
) -> Result<Value, Failure> {
    let item = part
        .definition
        .features
        .iter()
        .find(|f| f.id == feature)
        .ok_or_else(|| failure("selection", format!("unknown sketch feature '{feature}'")))?;
    let authored = match &item.operation {
        FeatureOperation::SketchFace { sketch }
        | FeatureOperation::SketchWire { sketch }
        | FeatureOperation::SketchOpenWire { sketch } => sketch,
        _ => {
            return Err(failure(
                "selection",
                format!("feature '{feature}' is not a sketch"),
            ));
        }
    };
    let parameters = part
        .resolved_parameters()
        .map_err(|e| model_failure("resolution", e))?;
    let session = Session::new().map_err(|e| failure("kernel", e))?;
    let materialized;
    let sketch = if authored.face_support.is_some() || !authored.projections.is_empty() {
        let family = subset(part.definition, feature, false)?;
        let sources = PartInstance {
            id: part.id.clone(),
            definition: &family,
            overrides: part.overrides.clone(),
            provenance: part.provenance.clone(),
        };
        let built = sources
            .regenerate(&session)
            .map_err(|e| model_failure("sketch_dependency", e))?;
        materialized = part
            .resolved_sketches(&session, &built)
            .map_err(|e| model_failure("sketch_dependency", e))?
            .remove(feature)
            .ok_or_else(|| failure("sketch_dependency", "linked sketch was not resolved"))?
            .map_err(|e| model_failure("sketch_dependency", e))?
            .sketch;
        &materialized
    } else {
        authored.as_ref()
    };
    let mut result = json!({"feature":feature,"coordinate_system":"sketch-local XY millimeters","definition":authored,"configuration":patch::sketch::SketchConfiguration::from(authored.as_ref()),
        "accepted":false,"scope":"isolated sketch diagnosis; publication still verifies all selected outputs and requirements",
        "constraints":page(&authored.constraints.iter().enumerate().map(|(index,expected)|json!({"index":index,"expected":expected})).collect::<Vec<_>>(),offset,limit),
        "geometry_checked":false,"dependency_geometry_generated":authored.face_support.is_some() || !authored.projections.is_empty()});
    let solution = match sketch.solve(&parameters) {
        Ok(s) => s,
        Err(e) => {
            result["error"] = json!({"stage":"sketch_solve","message":e.message,"diagnostics":report::diagnostics(&e.diagnostics)});
            return Ok(result);
        }
    };
    let points = solution
        .points
        .iter()
        .map(|(id, p)| (id.clone(), [p.x, p.y]))
        .collect::<BTreeMap<_, _>>();
    let acceptance = sketch.check_acceptance(&solution);
    let policy_ok = acceptance.is_ok();
    result["solver"] = json!({"solved":solution.solved,"accepted_by_sketch_policy":acceptance.is_ok(),"acceptance_error":acceptance.err().map(|e|e.message),"max_residual":solution.max_residual,"free_degrees":solution.free_degrees,"redundant_equations":solution.redundant_equations,"report":solution.solver_report,"diagnosis":solution.diagnosis});
    result["solved_points"] = json!(points);
    result["resolved_definition"] = json!(sketch);
    let checks = sketch
        .constraint_checks(&parameters, &solution)
        .map_err(|e| model_failure("sketch_solve", e))?;
    result["constraint_checks"]=page(&checks.iter().map(|c|json!({"index":c.index,"max_residual":c.max_residual,"satisfied":c.satisfied,"by_construction":c.by_construction})).collect::<Vec<_>>(),offset,limit);
    let mut repairs = Vec::new();
    for &index in solution
        .diagnosis
        .locally_redundant_constraints
        .iter()
        .take(128)
    {
        if let Some(expected) = authored.constraints.get(index) {
            repairs.push(json!({"kind":"remove_locally_redundant_constraint","requires_preview":true,"change":{"action":"remove_constraint","index":index,"expected":expected},"reason":"Local redundancy is not proof that removal preserves every parameter configuration."}));
        }
    }
    for group in solution.diagnosis.conflict_groups.iter().take(128) {
        repairs.push(json!({"kind":"review_conflict_group","requires_preview":true,"group":group,"reason":"Select an intended driving equation before replacing or removing constraints."}));
    }
    if solution.free_degrees > 0 {
        repairs.push(json!({"kind":"constrain_intended_free_motion","free_coordinates":solution.diagnosis.free_coordinates,"free_motions":solution.diagnosis.free_motions,"reason":"Choose design intent; no arbitrary fixed-point constraints are added."}));
    }
    result["repair_candidates"] = json!(repairs);
    if solution.solved && policy_ok {
        let family = subset(part.definition, feature, true)?;
        let isolated = PartInstance {
            id: part.id.clone(),
            definition: &family,
            overrides: part.overrides.clone(),
            provenance: part.provenance.clone(),
        };
        result["geometry_checked"] = json!(true);
        result["native_profile"] = match isolated.regenerate(&session) {
            Ok(built) => {
                json!({"valid":session.is_valid(built.shape(feature).ok_or_else(||failure("selection","isolated sketch output missing"))?).map_err(|e|failure("kernel",e))?})
            }
            Err(e) => {
                json!({"valid":false,"message":e.message,"diagnostics":report::diagnostics(&e.diagnostics)})
            }
        };
    }
    Ok(result)
}
