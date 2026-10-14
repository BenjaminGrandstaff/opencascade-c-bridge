//! One read-only native candidate snapshot per picked curve pair, never per hover.
use super::*;
use occt_parametric::{FeatureOperation, PartInstanceProjection, SketchDefinition};
impl Studio {
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
        let graph = document.instance_graph().map_err(|e| e.to_string())?;
        let part = graph.resolve(&query.instance).map_err(|e| e.to_string())?;
        let parameters = part.resolved_parameters().map_err(|e| e.to_string())?;
        let source = part
            .definition
            .features
            .iter()
            .find(|f| f.id == query.feature)
            .ok_or("unknown sketch feature")?;
        let sketch = match &source.operation {
            FeatureOperation::SketchFace { sketch }
            | FeatureOperation::SketchWire { sketch }
            | FeatureOperation::SketchOpenWire { sketch } => sketch,
            _ => return Err("feature has no authored sketch".into()),
        };
        let resolved;
        let sketch = if sketch.projections.is_empty() {
            sketch.as_ref()
        } else {
            let session = Session::new().map_err(|e| e.to_string())?;
            let geometry = part
                .diagnostic_geometry(&session)
                .map_err(|e| e.to_string())?;
            resolved = part
                .resolved_sketches(&session, &geometry.generated)
                .map_err(|e| e.to_string())?
                .remove(&query.feature)
                .ok_or("projected sketch unavailable")?
                .map_err(|e| e.to_string())?
                .sketch;
            &resolved
        };
        let points = sketch
            .finite_curve_intersections(&parameters, &query.first, &query.second)
            .map_err(|e| e.to_string())?;
        Ok(
            json!({"version":self.version,"instance":query.instance,"feature":query.feature,"first":query.first,"second":query.second,"points":points.into_iter().map(|p|[p.x,p.y]).collect::<Vec<_>>(),"complete":true,"maximum_candidates":1000,"coordinate_system":"sketch-local millimeters"}),
        )
    }
}
