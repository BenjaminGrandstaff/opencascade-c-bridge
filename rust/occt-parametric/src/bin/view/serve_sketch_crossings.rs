//! One read-only native candidate snapshot per picked curve pair, never per hover.
use super::*;
use occt_parametric::{FeatureOperation, PartInstanceProjection, SketchDefinition};
impl Studio {
    pub(super) fn sketch_spline_sample(&self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Query {
            version: u64,
            instance: String,
            feature: String,
            expected: SketchDefinition,
            definition: SketchDefinition,
            spline: String,
            position: [f64; 2],
            maximum_distance: f64,
        }
        let query: Query =
            serde_json::from_slice(body).map_err(|e| format!("invalid sample query: {e}"))?;
        if !query.maximum_distance.is_finite()
            || query.maximum_distance <= 0.
            || !query.position.iter().all(|v| v.is_finite())
        {
            return Err("sample pick needs a finite hint and positive distance tolerance".into());
        }
        let document = self.sketch_draft_document(
            query.version,
            &query.instance,
            &query.feature,
            &query.expected,
            &query.definition,
            true,
        )?;
        let (sketch, parameters) = query_sketch(&document, &query.instance, &query.feature)?;
        let (fraction, point) = sketch
            .closest_spline_parameter(
                &parameters,
                &query.spline,
                occt_parametric::SketchPoint2 {
                    x: query.position[0],
                    y: query.position[1],
                },
            )
            .map_err(|e| e.to_string())?;
        let distance = (point.x - query.position[0]).hypot(point.y - query.position[1]);
        if distance > query.maximum_distance {
            return Err("Pick closer to the source spline in the current native view.".into());
        }
        Ok(
            json!({"version":self.version,"instance":query.instance,"feature":query.feature,"spline":query.spline,"fraction":fraction,"point":[point.x,point.y],"distance":distance,"coordinate_system":"sketch-local millimeters"}),
        )
    }
    pub(super) fn sketch_crossing_candidates(&self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Query {
            version: u64,
            instance: String,
            feature: String,
            expected: SketchDefinition,
            definition: SketchDefinition,
            first: String,
            second: String,
        }
        let query: Query =
            serde_json::from_slice(body).map_err(|e| format!("invalid crossing query: {e}"))?;
        let document = self.sketch_draft_document(
            query.version,
            &query.instance,
            &query.feature,
            &query.expected,
            &query.definition,
            true,
        )?;
        let (sketch, parameters) = query_sketch(&document, &query.instance, &query.feature)?;
        let points = sketch
            .finite_curve_intersections(&parameters, &query.first, &query.second)
            .map_err(|e| e.to_string())?;
        Ok(
            json!({"version":self.version,"instance":query.instance,"feature":query.feature,"first":query.first,"second":query.second,"points":points.into_iter().map(|p|[p.x,p.y]).collect::<Vec<_>>(),"complete":true,"maximum_candidates":1000,"coordinate_system":"sketch-local millimeters"}),
        )
    }
}

// Resolve source projections once for read-only sketch geometry queries.
fn query_sketch(
    document: &ModelDocument,
    instance: &str,
    feature: &str,
) -> Result<(SketchDefinition, HashMap<String, ParameterValue>), String> {
    let graph = document.instance_graph().map_err(|e| e.to_string())?;
    let part = graph.resolve(instance).map_err(|e| e.to_string())?;
    let parameters = part.resolved_parameters().map_err(|e| e.to_string())?;
    let source = part
        .definition
        .features
        .iter()
        .find(|f| f.id == feature)
        .ok_or("unknown sketch feature")?;
    let sketch = match &source.operation {
        FeatureOperation::SketchFace { sketch }
        | FeatureOperation::SketchWire { sketch }
        | FeatureOperation::SketchOpenWire { sketch } => sketch,
        _ => return Err("feature has no authored sketch".into()),
    };
    let sketch = if sketch.projections.is_empty() {
        sketch.as_ref().clone()
    } else {
        let session = Session::new().map_err(|e| e.to_string())?;
        let geometry = part
            .diagnostic_geometry(&session)
            .map_err(|e| e.to_string())?;
        part.resolved_sketches(&session, &geometry.generated)
            .map_err(|e| e.to_string())?
            .remove(feature)
            .ok_or("projected sketch unavailable")?
            .map_err(|e| e.to_string())?
            .sketch
    };
    Ok((sketch, parameters))
}
