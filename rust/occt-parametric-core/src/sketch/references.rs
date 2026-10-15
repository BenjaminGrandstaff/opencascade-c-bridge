//! Structurally admissible generated names, without solving coordinates.
use super::*;
use std::collections::BTreeMap;

/// Read-only structural inspection metadata, outside the persisted model format.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SketchGeneratedReferences {
    pub points: Vec<String>,
    /// Curve names grouped by native kind: lines, circles, arcs, ellipses, splines.
    pub curves: BTreeMap<String, Vec<String>>,
}
impl SketchDefinition {
    /// Generated shape/pattern point IDs that can be referenced by authored
    /// geometry and constraints. Projection poles are not stable authored IDs;
    /// only declared projection endpoints and named points participate in copies.
    /// Expansion uses the core shape/pattern budgets and never solves geometry.
    pub fn generated_point_references(&self) -> Result<Vec<String>, ModelError> {
        Ok(self.generated_references()?.points)
    }

    /// Stable shape/pattern point and curve names, grouped by native curve kind.
    /// Names refer to source geometry before profile edits; split/trim output
    /// pieces are not solver entities. Expansion does not evaluate parameters.
    pub fn generated_references(&self) -> Result<SketchGeneratedReferences, ModelError> {
        self.validate_structure()?;
        let seeded = if self.projections.is_empty() {
            std::borrow::Cow::Borrowed(self)
        } else {
            std::borrow::Cow::Owned(self.projection_seed()?)
        };
        let original = seeded
            .points
            .iter()
            .map(|p| p.id.as_str())
            .collect::<HashSet<_>>();
        let expanded = seeded.with_patterns(None)?;
        let mut ids = expanded
            .points
            .iter()
            .filter(|p| !original.contains(p.id.as_str()))
            .map(|p| p.id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        let original_curves = seeded.entities();
        let mut curves: BTreeMap<String, Vec<String>> =
            ["lines", "circles", "arcs", "ellipses", "splines"]
                .into_iter()
                .map(|kind| (kind.to_string(), Vec::new()))
                .collect();
        for (id, entity) in expanded.entities() {
            if original_curves.contains_key(id) {
                continue;
            }
            let kind = match entity {
                Entity::Line(_) => "lines",
                Entity::Circle(_) => "circles",
                Entity::Arc(_) => "arcs",
                Entity::Ellipse(_) => "ellipses",
                Entity::Spline(_) => "splines",
            };
            curves.get_mut(kind).unwrap().push(id.to_string());
        }
        for names in curves.values_mut() {
            names.sort();
        }
        Ok(SketchGeneratedReferences {
            points: ids,
            curves,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn fixture() -> Value {
        let request: Value = serde_json::from_str(include_str!(
            "../../../../tools/model/slotted-plate.request.json"
        ))
        .unwrap();
        request["model"]["family"]["features"][0]["operation"]["sketch_face"]["sketch"].clone()
    }
    fn ids(value: &Value) -> Result<Vec<String>, ModelError> {
        serde_json::from_value::<SketchDefinition>(value.clone())
            .unwrap()
            .generated_point_references()
    }
    #[test]
    fn shape_catalogue_is_sorted_stable_and_does_not_evaluate_parameters() {
        let value = fixture();
        let names = ids(&value).unwrap();
        assert_eq!(names.len(), 15);
        for name in [
            "outline:corner1",
            "outline:corner3",
            "lower_slot:start_right",
            "upper_slot:end_left",
            "hex:vertex5",
        ] {
            assert!(names.contains(&name.to_string()), "{name}");
        }
        assert!(!names.contains(&"a".to_string()));
        assert!(names.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(ids(&value).unwrap(), names);
    }
    #[test]
    fn catalogue_expands_chained_patterns_and_shared_mirror_points() {
        let mut value = fixture();
        value["lines"] =
            json!([{"id":"axis","start":"a","end":"c"},{"id":"guide","start":"a","end":"hv"}]);
        value["patterns"] = json!([
            {"mirror":{"id":"mirror","entities":["guide"],"axis":"axis","on_axis":["a"]}},
            {"linear":{"id":"row","entities":["outline:side1"],"direction":"axis","spacing":{"parameter":"pitch"},"count":3}},
            {"circular":{"id":"ring","entities":["row:2:outline:side1"],"center":"hc","count":2}}
        ]);
        let names = ids(&value).unwrap();
        for name in [
            "mirror:1:hv",
            "row:1:c",
            "row:2:outline:corner1",
            "ring:1:row:2:c",
        ] {
            assert!(names.contains(&name.to_string()), "{name}");
        }
        assert!(!names.contains(&"mirror:1:a".to_string()));
        value["patterns"][1]["linear"]["count"] = json!(2);
        assert!(ids(&value).is_err(), "later pattern lost its source");
    }
    #[test]
    fn copied_projections_expose_stable_endpoints_without_native_poles() {
        let mut value = fixture();
        let request: Value = serde_json::from_str(include_str!(
            "../../../../tools/model/projected-spline-refinement.request.json"
        ))
        .unwrap();
        let projected = request["model"]["family"]["features"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|f| {
                f["operation"]
                    .as_object()
                    .unwrap()
                    .values()
                    .find_map(|op| op.get("sketch").filter(|s| s.get("projections").is_some()))
            })
            .unwrap();
        value["projections"] = projected["projections"].clone();
        value["lines"] = json!([{"id":"axis","start":"a","end":"c"}]);
        value["patterns"] = json!([{ "linear":{"id":"row","entities":["reference"],"direction":"axis","spacing":{"parameter":"pitch"},"count":2}}]);
        let names = ids(&value).unwrap();
        assert!(names.contains(&"row:1:reference:start".to_string()));
        assert!(names.contains(&"row:1:reference:end".to_string()));
        assert!(
            !names
                .iter()
                .any(|id| id.contains(":pole:") || id == "reference:start")
        );
    }
    #[test]
    fn generated_curves_keep_native_kinds_and_chained_copy_names() {
        let mut value = fixture();
        value["circles"] = json!([{"id":"circle","center":"hc","rim":"hv"}]);
        value["arcs"] = json!([{"id":"arc","center":"hc","start":"hv","end":"s1a"}]);
        value["ellipses"] = json!([{"id":"ellipse","center":"hc","major":"hv","minor":"s1a"}]);
        value["splines"] = json!([{"id":"spline","points":["hc","hv","s1a"],"degree":2}]);
        value["patterns"] = json!([
            {"linear":{"id":"row","entities":["outline:side1","circle","arc","ellipse","spline"],"direction":"outline:side0","spacing":{"parameter":"pitch"},"count":2}},
            {"circular":{"id":"ring","entities":["row:1:ellipse"],"center":"a","count":2}}
        ]);
        let sketch: SketchDefinition = serde_json::from_value(value).unwrap();
        let refs = sketch.generated_references().unwrap();
        for (kind, name) in [
            ("lines", "outline:side0"),
            ("lines", "row:1:outline:side1"),
            ("circles", "hex:circle"),
            ("circles", "row:1:circle"),
            ("arcs", "lower_slot:end_cap"),
            ("arcs", "row:1:arc"),
            ("ellipses", "row:1:ellipse"),
            ("ellipses", "ring:1:row:1:ellipse"),
            ("splines", "row:1:spline"),
        ] {
            assert!(
                refs.curves[kind].contains(&name.to_string()),
                "{kind}: {name}"
            );
        }
        assert!(
            !refs
                .curves
                .values()
                .flatten()
                .any(|id| ["circle", "arc", "ellipse", "spline"].contains(&id.as_str()))
        );
        assert!(
            refs.curves
                .values()
                .all(|ids| ids.windows(2).all(|w| w[0] < w[1]))
        );
        assert_eq!(refs.points, sketch.generated_point_references().unwrap());
    }
    #[test]
    fn invalid_generators_and_expansion_budgets_reject_catalogues() {
        let mut value = fixture();
        value["shapes"][0]["rectangle"]["opposite"] = json!("missing");
        assert!(ids(&value).is_err());
        let mut value = fixture();
        value["shapes"]=json!((0..21).map(|i|json!({"polygon":{"id":format!("p{i}"),"center":"hc","vertex":"hv","sides":1000}})).collect::<Vec<_>>());
        assert!(ids(&value).is_err());
    }
}
