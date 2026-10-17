//! Read-only source views and semantic edge picks for existing sketch drafts.
use super::*;
use occt_bridge::{AnalyticCurve, ShapeType};
use occt_parametric::{
    CoordinateAxis, EdgeSelector, Extremum, Quantity, ScalarExpr, SketchProjection,
    SketchProjectionKind, VectorExpr,
};

impl Studio {
    pub(super) fn sketch_source_view(&self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Source {
            version: u64,
            instance: String,
            output: String,
        }
        let source: Source =
            serde_json::from_slice(body).map_err(|e| format!("invalid source view: {e}"))?;
        if source.version != self.version {
            return Err("Model changed. Reload the sketch draft.".into());
        }
        if !primary_instances(&self.document)?.contains(&source.instance) {
            return Err("Unknown editable instance.".into());
        }
        if !self
            .document
            .family
            .features
            .iter()
            .any(|f| f.id == source.output)
        {
            return Err("Unknown source feature.".into());
        }
        let outputs = [occt_parametric::InstanceOutputRef {
            instance: source.instance,
            output: source.output,
        }];
        let mut data = view_data::collect(
            &self.document,
            &outputs,
            true,
            &view_data::ViewOptions::default(),
        )
        .map_err(|e| e.to_string())?;
        data["version"] = json!(self.version);
        data["accepted"] = json!(true);
        data["preview"] = json!(true);
        data["source_view"] = json!(true);
        Ok(data)
    }

    pub(super) fn sketch_edge_pick(&self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Pick {
            version: u64,
            instance: String,
            output: String,
            edge: usize,
            id: String,
        }
        let pick: Pick =
            serde_json::from_slice(body).map_err(|e| format!("invalid edge pick: {e}"))?;
        if pick.version != self.version {
            return Err("Model changed. Reload the sketch draft.".into());
        }
        if !primary_instances(&self.document)?.contains(&pick.instance) {
            return Err("Unknown editable instance.".into());
        }
        if pick.id.trim().is_empty() || pick.id != pick.id.trim() {
            return Err("Enter a projection name without surrounding spaces.".into());
        }
        let mut graph = self.document.instance_graph().map_err(|e| e.to_string())?;
        let session = Session::new().map_err(|e| e.to_string())?;
        let generation = graph.regenerate_all(&session).map_err(|e| e.to_string())?;
        let result = generation
            .instances()
            .find(|(id, _)| *id == pick.instance)
            .map(|(_, r)| r)
            .ok_or("Instance unavailable.")?;
        let shape = result
            .shape(&pick.output)
            .ok_or("Source output unavailable.")?;
        let edge = session
            .subshape(shape, ShapeType::Edge, pick.edge)
            .map_err(|e| e.to_string())?;
        let centre = session.center_of_mass(&edge).map_err(|e| e.to_string())?;
        let length = session.edge_length(&edge).map_err(|e| e.to_string())?;
        let kind = match session
            .edge_analytic_curve(&edge)
            .map_err(|e| e.to_string())?
        {
            Some(AnalyticCurve::Line { .. }) => SketchProjectionKind::Line,
            _ => SketchProjectionKind::Spline,
        };
        let part = graph.resolve(&pick.instance).map_err(|e| e.to_string())?;
        let mut remaining = 128usize;
        let mut matches = |selector: &EdgeSelector| {
            if remaining == 0 {
                return false;
            }
            remaining -= 1;
            part.select_edges(&session, result, &pick.output, selector)
                .ok()
                .is_some_and(|edges| {
                    edges.len() == 1 && session.is_same(&edges[0], &edge).unwrap_or(false)
                })
        };
        // Kernel bounds include padding; leave room beyond its 1e-7 scale.
        // A candidate is still published only after unique identity matching.
        let extremes: Vec<_> = [CoordinateAxis::X, CoordinateAxis::Y, CoordinateAxis::Z]
            .into_iter()
            .flat_map(|axis| {
                [Extremum::Minimum, Extremum::Maximum]
                    .into_iter()
                    .map(move |extremum| EdgeSelector::AtExtreme {
                        axis,
                        extremum,
                        tolerance: ScalarExpr::Literal(Quantity::length(
                            1e-6,
                            LengthUnit::Millimeter,
                        )),
                    })
            })
            .collect();
        let mut queries = extremes.clone();
        for a in 0..extremes.len() {
            for b in a + 1..extremes.len() {
                queries.push(EdgeSelector::Intersection(vec![
                    extremes[a].clone(),
                    extremes[b].clone(),
                ]));
            }
        }
        let mut selected = None;
        for candidate in &queries {
            if matches(candidate) {
                selected = Some((candidate.clone(), "extreme"));
                break;
            }
        }
        if selected.is_none() {
            for feature in self
                .document
                .family
                .features
                .iter()
                .rev()
                .filter(|f| f.id != pick.output)
                .take(16)
            {
                // Authored curve identities precede geometry-only queries.
                let operation =
                    serde_json::to_value(&feature.operation).map_err(|e| e.to_string())?;
                let mut candidates = Vec::new();
                if let Some(sketch) = operation
                    .as_object()
                    .into_iter()
                    .flat_map(|v| v.values())
                    .find_map(|v| v.get("sketch"))
                {
                    for key in [
                        "lines",
                        "circles",
                        "arcs",
                        "ellipses",
                        "splines",
                        "projections",
                    ] {
                        for entity in sketch[key].as_array().into_iter().flatten().take(32) {
                            if let Some(id) = entity["id"].as_str() {
                                candidates.push(EdgeSelector::SketchCurve {
                                    entity: id.into(),
                                    require_complete: true,
                                });
                            }
                        }
                    }
                }
                candidates.extend(queries.iter().cloned());
                for selector in candidates {
                    let candidate = EdgeSelector::Persistent {
                        feature: feature.id.clone(),
                        select: Box::new(selector),
                    };
                    if matches(&candidate) {
                        selected = Some((candidate, "history"));
                        break;
                    }
                }
                if selected.is_some() {
                    break;
                }
            }
        }
        let (selector, binding) = if let Some(selected) = selected {
            selected
        } else {
            let selector = EdgeSelector::NearestCenter {
                target: VectorExpr::Literal(VectorQuantity::lengths(
                    centre.x,
                    centre.y,
                    centre.z,
                    LengthUnit::Millimeter,
                )),
                maximum_distance: ScalarExpr::Literal(Quantity::length(
                    1e-5,
                    LengthUnit::Millimeter,
                )),
            };
            // Check outside the ancestry budget: never publish an unverified anchor.
            let edges = part
                .select_edges(&session, result, &pick.output, &selector)
                .map_err(|e| e.to_string())?;
            if edges.len() != 1
                || !session
                    .is_same(&edges[0], &edge)
                    .map_err(|e| e.to_string())?
            {
                return Err("No unique semantic edge selector.".into());
            }
            (selector, "anchored_center")
        };
        let projection = SketchProjection {
            id: pick.id,
            input: pick.output,
            edge: selector,
            kind,
            junctions: Vec::new(),
            intersections: Vec::new(),
            refinements: Vec::new(),
        };
        Ok(
            json!({"version":self.version,"instance":pick.instance,"projection":projection,"binding":binding,"length_mm":length,"center_mm":[centre.x,centre.y,centre.z]}),
        )
    }
}
