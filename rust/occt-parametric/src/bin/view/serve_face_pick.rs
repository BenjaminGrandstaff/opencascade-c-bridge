//! Mesh indices are version-local hints. Only semantic selectors are authored.
use super::*;
use occt_bridge::{Shape, ShapeType};
use occt_parametric::{FaceSelector, Quantity, ScalarExpr, SketchFaceSupport, VectorExpr};

impl Studio {
    pub(super) fn pick_support_face(&self, body: &[u8]) -> Result<Value, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Pick {
            version: u64,
            instance: String,
            output: String,
            face: usize,
        }
        let pick: Pick =
            serde_json::from_slice(body).map_err(|e| format!("invalid face pick: {e}"))?;
        if pick.version != self.version {
            return Err("The model changed. Reload New sketch and pick the face again.".into());
        }
        if !primary_instances(&self.document)?.contains(&pick.instance) {
            return Err("Unknown editable instance.".into());
        }
        let mut graph = self.document.instance_graph().map_err(|e| e.to_string())?;
        let session = Session::new().map_err(|e| e.to_string())?;
        let generation = graph.regenerate_all(&session).map_err(|e| e.to_string())?;
        let result = generation
            .instances()
            .find(|(id, _)| *id == pick.instance)
            .map(|(_, result)| result)
            .ok_or("Picked instance is unavailable.")?;
        let shape = result
            .shape(&pick.output)
            .ok_or("Picked output is unavailable.")?;
        let face = session
            .subshape(shape, ShapeType::Face, pick.face)
            .map_err(|e| e.to_string())?;
        if !session.face_is_planar(&face).map_err(|e| e.to_string())? {
            return Err(
                "Pick a planar face; curved faces are not supported by this picker.".into(),
            );
        }
        let centre = session.center_of_mass(&face).map_err(|e| e.to_string())?;
        let n = session.face_normal(&face).map_err(|e| e.to_string())?;
        let norm = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
        if !norm.is_finite() || norm <= 0.0 {
            return Err("Face normal is unavailable.".into());
        }
        let normal = [n.x / norm, n.y / norm, n.z / norm];
        let direction =
            |v: [f64; 3]| VectorExpr::Literal(VectorQuantity::scalars(v[0], v[1], v[2]));
        let aligned = |sign: f64| FaceSelector::NormalAligned {
            direction: direction(normal.map(|v| v * sign)),
            minimum_dot: ScalarExpr::Literal(Quantity::scalar(0.999999)),
        };
        let part = graph.resolve(&pick.instance).map_err(|e| e.to_string())?;
        let matches = |selector: &FaceSelector| -> Result<bool, String> {
            let chosen: Vec<Shape<'_>> = part
                .select_faces(&session, result, &pick.output, selector)
                .map_err(|e| e.to_string())?;
            Ok(chosen.len() == 1
                && session
                    .is_same(&chosen[0], &face)
                    .map_err(|e| e.to_string())?)
        };
        let mut selector = aligned(1.0);
        let mut binding = "normal";
        if !matches(&selector)? {
            let mut found = None;
            // Prefer a unique ancestral face over a literal coordinate anchor.
            // Bound selector work; very large graphs can use the guarded anchor.
            for feature in self
                .document
                .family
                .features
                .iter()
                .rev()
                .filter(|f| f.id != pick.output)
                .take(64)
            {
                for sign in [1.0, -1.0] {
                    let candidate = FaceSelector::Persistent {
                        feature: feature.id.clone(),
                        select: Box::new(aligned(sign)),
                    };
                    if matches(&candidate).unwrap_or(false) {
                        found = Some(candidate);
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            if let Some(candidate) = found {
                selector = candidate;
                binding = "history";
            } else {
                // Strict stationary anchor: moving/disappearing support fails
                // rather than selecting an unrelated nearby parallel face.
                selector = FaceSelector::Intersection(vec![
                    aligned(1.0),
                    FaceSelector::NearestCenter {
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
                    },
                ]);
                if !matches(&selector)? {
                    return Err("This face has no unambiguous semantic support selector.".into());
                }
                binding = "anchored_center";
            }
        }
        let axis = (0..3)
            .min_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs()))
            .unwrap();
        let mut x = normal.map(|v| -normal[axis] * v);
        x[axis] += 1.0;
        let length = x.iter().map(|v| v * v).sum::<f64>().sqrt();
        x = x.map(|v| v / length);
        let y = [
            normal[1] * x[2] - normal[2] * x[1],
            normal[2] * x[0] - normal[0] * x[2],
            normal[0] * x[1] - normal[1] * x[0],
        ];
        let support = SketchFaceSupport {
            input: pick.output,
            face: selector,
            offset: ScalarExpr::Literal(Quantity::length(0.0, LengthUnit::Millimeter)),
        };
        Ok(
            json!({"version":self.version,"instance":pick.instance,"face_support":support,
            "origin_mm":[centre.x,centre.y,centre.z],"normal":normal,
            "x_axis":direction(x),"y_axis":direction(y),"binding":binding}),
        )
    }
}
