//! Bounded, read-only authoring and regenerated geometry inspection.
use super::*;
use occt_bridge::{Shape, ShapeType, Vec3};
use serde::Serialize;
use std::{collections::BTreeMap, fs::OpenOptions, io::Write};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Inspection {
    schema: String,
    model: ModelDocument,
    #[serde(default)]
    instance: Option<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    face_selector: Option<FaceSelector>,
    #[serde(default)]
    edge_selector: Option<EdgeSelector>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    100
}
fn page<T: Serialize>(items: &[T], offset: usize, limit: usize) -> Value {
    let end = offset.saturating_add(limit).min(items.len());
    let start = offset.min(items.len());
    json!({"total":items.len(),"offset":offset,"next_offset":(end < items.len()).then_some(end),"items":&items[start..end]})
}
fn vector(p: Vec3) -> [f64; 3] {
    [p.x, p.y, p.z]
}
fn bounds(session: &Session, shape: &Shape<'_>) -> Result<Value, Failure> {
    let b = session
        .exact_bounds(shape)
        .map_err(|e| failure("inspection", e))?;
    Ok(json!({"min":vector(b.min),"max":vector(b.max)}))
}

pub fn run(path: &OsString, destination: &OsString) -> Result<Value, Failure> {
    let mut raw: Value =
        serde_json::from_str(&fs::read_to_string(path).map_err(|e| failure("request", e))?)
            .map_err(|e| failure("request", e))?;
    let model = ModelDocument::from_json(
        &raw.get("model")
            .ok_or_else(|| failure("request", "model is required"))?
            .to_string(),
    )
    .map_err(|e| model_failure("validation", e))?;
    raw["model"] = serde_json::to_value(model).map_err(|e| failure("request", e))?;
    let request: Inspection = serde_json::from_value(raw).map_err(|e| failure("request", e))?;
    let result = inspect(request)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| failure("publication", e))?;
    file.write_all(
        serde_json::to_string(&result)
            .map_err(|e| failure("publication", e))?
            .as_bytes(),
    )
    .map_err(|e| failure("publication", e))?;
    Ok(result)
}
fn inspect(request: Inspection) -> Result<Value, Failure> {
    if request.schema != "occb-model-inspection-v1" || !(1..=1000).contains(&request.limit) {
        return Err(failure(
            "request",
            "expected occb-model-inspection-v1 and a limit in 1..1000",
        ));
    }
    if (request.output.is_some() && request.instance.is_none())
        || ((request.face_selector.is_some() || request.edge_selector.is_some())
            && request.output.is_none())
    {
        return Err(failure(
            "request",
            "output inspection requires an instance; selectors require an output",
        ));
    }
    let document = &request.model;
    let mut graph = document
        .instance_graph()
        .map_err(|e| model_failure("validation", e))?;
    let mut nodes = document.instances.iter().collect::<Vec<_>>();
    nodes.sort_by_key(|node| node.id());
    let mut families = std::iter::once(&document.family).chain(&document.additional_families)
        .map(|family| json!({"id":family.id,"version":family.version,"parameters":family.parameters.len(),"features":family.features.len(),"requirements":family.requirements.len()})).collect::<Vec<_>>();
    families.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let mut result = json!({"schema":"occb-model-inspection-report-v1","status":"inspected","model_schema":CURRENT_SCHEMA_VERSION,
        "geometry_generated":false,"inventory_scope":"saved declarations; driven members refresh only for geometry inspection","instances":page(&nodes,request.offset,request.limit),"families":page(&families,request.offset,request.limit),
        "patterns":page(&document.patterns,request.offset,request.limit),"assembly_requirements":page(&document.assembly.requirements,request.offset,request.limit)});
    let Some(id) = &request.instance else {
        return Ok(result);
    };
    // Geometry regeneration refreshes driven pattern members before resolution.
    let session = request
        .output
        .as_ref()
        .map(|_| Session::new())
        .transpose()
        .map_err(|e| failure("kernel", e))?;
    let generation = session
        .as_ref()
        .map(|session| graph.regenerate_all(session))
        .transpose()
        .map_err(|e| model_failure("regeneration", e))?;
    let resolved = graph
        .resolve_with_placement(id)
        .map_err(|e| model_failure("resolution", e))?;
    let part = &resolved.instance;
    let family = part.definition;
    let parameters = part
        .resolved_parameters()
        .map_err(|e| model_failure("resolution", e))?
        .into_iter()
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .map(|(id, value)| json!({"id":id,"value":value}))
        .collect::<Vec<_>>();
    let inputs = family
        .feature_inputs()
        .map_err(|e| model_failure("validation", e))?;
    let feature_info = family.features.iter().map(|feature|json!({"id":feature.id,"inputs":inputs[feature.id.as_str()],"operation":feature.operation})).collect::<Vec<_>>();
    result["instance"] = json!({"id":id,"family":family.id,"family_version":family.version,"provenance":part.provenance,
        "placement":resolved.placement,"frame_chain":resolved.frames,"inherited_overrides":part.overrides,
        "resolved_parameters":page(&parameters,request.offset,request.limit),"parameter_definitions":page(&family.parameters,request.offset,request.limit),
        "derived_parameters":page(&family.derived_parameters,request.offset,request.limit),"derived_vector_parameters":page(&family.derived_vector_parameters,request.offset,request.limit),
        "features":page(&feature_info,request.offset,request.limit),"requirements":page(&family.requirements,request.offset,request.limit),
        "constraints":page(&family.constraints,request.offset,request.limit),"datums":page(&family.datums,request.offset,request.limit),
        "references":page(&family.references,request.offset,request.limit),"assumptions":page(&family.assumptions,request.offset,request.limit)});
    if let (Some(output), Some(session), Some(generation)) =
        (&request.output, &session, &generation)
    {
        let generated = generation.result(id).ok_or_else(|| {
            failure(
                "selection",
                format!("instance '{id}' is suppressed or not generated"),
            )
        })?;
        let _ = generated
            .shape(output)
            .ok_or_else(|| failure("selection", format!("unknown generated output '{output}'")))?;
        // Inspect the family-local snapshot used when defining feature selectors.
        // Report assembly placement separately so selectors can be reused directly.
        let authoring = part
            .regenerate(session)
            .map_err(|e| model_failure("regeneration", e))?;
        let generated = &authoring;
        let shape = generated
            .shape(output)
            .ok_or_else(|| failure("selection", format!("unknown generated output '{output}'")))?;
        result["geometry"] = geometry(session, part, generated, shape, output, &request)?;
        result["geometry_generated"] = json!(true);
        result["verification"] = report::verification(generation);
        result["generated_variants"] = json!(generation.generated_variants());
        result["authoring_regenerations"] = json!(1);
    }
    Ok(result)
}
fn geometry(
    session: &Session,
    part: &PartInstance<'_>,
    generated: &GeneratedResult<'_>,
    shape: &Shape<'_>,
    output: &str,
    request: &Inspection,
) -> Result<Value, Failure> {
    let faces = match &request.face_selector {
        Some(selector) => part
            .select_faces(session, generated, output, selector)
            .map_err(|e| model_failure("selection", e))?,
        None => session
            .subshapes(shape, ShapeType::Face)
            .map_err(|e| failure("inspection", e))?,
    };
    let edges = match &request.edge_selector {
        Some(selector) => part
            .select_edges(session, generated, output, selector)
            .map_err(|e| model_failure("selection", e))?,
        None => session
            .subshapes(shape, ShapeType::Edge)
            .map_err(|e| failure("inspection", e))?,
    };
    let mut face_info = Vec::new();
    for (index, face) in faces
        .iter()
        .enumerate()
        .skip(request.offset)
        .take(request.limit)
    {
        let planar = session
            .face_is_planar(face)
            .map_err(|e| failure("inspection", e))?;
        face_info.push(json!({"selection_index":index,"area_mm2":session.surface_area(face).map_err(|e|failure("inspection",e))?,
            "center_mm":vector(session.center_of_mass(face).map_err(|e|failure("inspection",e))?),"bounds_mm":bounds(session,face)?,
            "planar":planar,"normal":if planar {Some(vector(session.face_normal(face).map_err(|e|failure("inspection",e))?))} else {None}}));
    }
    let mut edge_info = Vec::new();
    for (index, edge) in edges
        .iter()
        .enumerate()
        .skip(request.offset)
        .take(request.limit)
    {
        edge_info.push(json!({"selection_index":index,"length_mm":session.edge_length(edge).map_err(|e|failure("inspection",e))?,
            "center_mm":vector(session.center_of_mass(edge).map_err(|e|failure("inspection",e))?),"bounds_mm":bounds(session,edge)?,
            "circle_radius_mm":session.edge_circle_radius(edge).map_err(|e|failure("inspection",e))?}));
    }
    let page_info = |count: usize, items: Vec<Value>| json!({"total":count,"offset":request.offset,"next_offset":(request.offset.saturating_add(request.limit) < count).then_some(request.offset.saturating_add(request.limit)),"items":items});
    Ok(
        json!({"instance":part.id,"output":output,"coordinate_system":"family-local feature authoring coordinates; instance placement/frame chain reported separately",
        "shape_type":format!("{:?}",session.shape_type(shape).map_err(|e|failure("inspection",e))?),
        "valid":session.is_valid(shape).map_err(|e|failure("inspection",e))?,"volume_mm3":session.volume(shape).map_err(|e|failure("inspection",e))?,
        "bounds_mm":bounds(session,shape)?,"faces":page_info(faces.len(),face_info),"edges":page_info(edges.len(),edge_info),
        "face_selector":request.face_selector,"edge_selector":request.edge_selector,
        "index_scope":"selection-local for this regenerated snapshot; use semantic selectors or named references for edits"}),
    )
}
