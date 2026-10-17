//! Read-only exact boundary snapshots and snapped anchors; no topology indices are saved.
use super::*;
use occt_bridge::{ShapeType, Vec3};

impl Studio {
    pub(super) fn loft_profile_pick(&self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Query {
            version: u64,
            instance: String,
            profile: String,
            position_mm: Option<[f64; 3]>,
            maximum_distance_mm: Option<f64>,
            #[serde(default)]
            split_points: Vec<occt_parametric::VectorExpr>,
        }
        let query: Query =
            serde_json::from_slice(body).map_err(|e| format!("invalid loft profile pick: {e}"))?;
        if query.split_points.len() > 128 {
            return Err("A profile supports at most 128 split points.".into());
        }
        if query.version != self.version {
            return Err("Model changed. Pick the loft anchor again.".into());
        }
        if !primary_instances(&self.document)?.contains(&query.instance) {
            return Err("Unknown editable instance.".into());
        }
        match (query.position_mm, query.maximum_distance_mm) {
            (None, None) => {}
            (Some(p), Some(d)) if p.iter().all(|v| v.is_finite()) && d.is_finite() && d > 0. => {}
            _ => {
                return Err(
                    "A pick needs finite family coordinates and a positive distance tolerance."
                        .into(),
                );
            }
        }
        let mut graph = self.document.instance_graph().map_err(|e| e.to_string())?;
        let session = Session::new().map_err(|e| e.to_string())?;
        let generation = graph.regenerate_all(&session).map_err(|e| e.to_string())?;
        let result = generation
            .instances()
            .find(|(id, _)| *id == query.instance)
            .map(|(_, r)| r)
            .ok_or("Instance unavailable.")?;
        let source = result.shape(&query.profile).ok_or("Profile unavailable.")?;
        let extracted;
        let wire = match session.shape_type(source).map_err(|e| e.to_string())? {
            ShapeType::Wire => source,
            ShapeType::Face
                if session.face_is_planar(source).map_err(|e| e.to_string())?
                    && session
                        .subshape_count(source, ShapeType::Wire)
                        .map_err(|e| e.to_string())?
                        == 1 =>
            {
                extracted = session
                    .subshape(source, ShapeType::Wire, 0)
                    .map_err(|e| e.to_string())?;
                &extracted
            }
            _ => return Err("Choose a single-boundary closed planar profile.".into()),
        };
        let partitioned;
        let wire = if query.split_points.is_empty() {
            wire
        } else {
            let part = graph.resolve(&query.instance).map_err(|e| e.to_string())?;
            let parameters = part.resolved_parameters().map_err(|e| e.to_string())?;
            let points = query
                .split_points
                .iter()
                .map(|p| {
                    occt_parametric::vector(p, &parameters, occt_parametric::Dimension::Length)
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            partitioned = session
                .partition_profile_wire(wire, &points)
                .map_err(|e| e.to_string())?;
            &partitioned
        };
        let aligned = session
            .align_profile_wire(wire, None, false)
            .map_err(|e| e.to_string())?;
        let face = session
            .create_face_from_wire(&aligned)
            .map_err(|e| e.to_string())?;
        let origin = session.center_of_mass(&face).map_err(|e| e.to_string())?;
        let normal = session.face_normal(&face).map_err(|e| e.to_string())?;
        let edges = session
            .subshapes(&aligned, ShapeType::Edge)
            .map_err(|e| e.to_string())?;
        let to_array = |p: Vec3| [p.x, p.y, p.z];
        let mut vertices = Vec::<Vec3>::new();
        for edge in &edges {
            for t in [0., 1.] {
                let p = session
                    .curve_point_at_fraction(edge, t)
                    .map_err(|e| e.to_string())?;
                if !vertices
                    .iter()
                    .any(|v| (v.x - p.x).hypot((v.y - p.y).hypot(v.z - p.z)) < 1e-6)
                {
                    vertices.push(p);
                }
            }
        }
        // The same native aligner decides whether a continuous nonvertex seam is supported.
        let continuous = edges.len() == 1
            && session
                .curve_point_at_fraction(&edges[0], 0.5)
                .is_ok_and(|p| session.align_profile_wire(&aligned, Some(p), false).is_ok());
        let mut data = json!({"version":self.version,"instance":query.instance,"profile":query.profile,"accepted":true,"coordinate_system":"family-local mm","continuous":continuous,"split_count":query.split_points.len()});
        if let Some(position) = query.position_mm {
            let hint = Vec3::new(position[0], position[1], position[2]);
            let anchor = if continuous {
                session
                    .curve_closest_point(&edges[0], hint)
                    .map_err(|e| e.to_string())?
            } else {
                *vertices
                    .iter()
                    .min_by(|a, b| {
                        let distance =
                            |v: &&Vec3| (v.x - hint.x).hypot((v.y - hint.y).hypot(v.z - hint.z));
                        distance(a).total_cmp(&distance(b))
                    })
                    .ok_or("Profile has no boundary vertices.")?
            };
            let distance = (anchor.x - hint.x).hypot((anchor.y - hint.y).hypot(anchor.z - hint.z));
            if distance > query.maximum_distance_mm.ok_or("Missing pick tolerance.")? {
                return Err("Pick closer to a supported boundary start.".into());
            }
            let _verified = session
                .align_profile_wire(&aligned, Some(anchor), false)
                .map_err(|e| e.to_string())?;
            data["position_mm"] = json!(to_array(anchor));
            data["distance_mm"] = json!(distance);
        } else {
            let mut lines = Vec::new();
            for edge in &edges {
                let points = session
                    .edge_sample_points(edge, if continuous { 257 } else { 65 })
                    .map_err(|e| e.to_string())?;
                lines.push(points.into_iter().map(to_array).collect::<Vec<_>>());
            }
            data["vertices"] = json!(vertices.into_iter().map(to_array).collect::<Vec<_>>());
            data["lines"] = json!(lines);
            data["origin_mm"] = json!(to_array(origin));
            data["normal"] = json!(to_array(normal));
        }
        Ok(data)
    }
}
