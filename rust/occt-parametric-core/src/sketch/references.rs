//! Structurally admissible generated point names, without solving coordinates.
use super::*;
impl SketchDefinition {
    /// Generated shape/pattern point IDs that can be referenced by authored
    /// geometry and constraints. Projection poles are not stable authored IDs;
    /// only declared projection endpoints and named points participate in copies.
    /// Expansion uses the core shape/pattern budgets and never solves geometry.
    pub fn generated_point_references(&self) -> Result<Vec<String>, ModelError> {
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
        Ok(ids)
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
    fn invalid_generators_and_expansion_budgets_reject_catalogues() {
        let mut value = fixture();
        value["shapes"][0]["rectangle"]["opposite"] = json!("missing");
        assert!(ids(&value).is_err());
        let mut value = fixture();
        value["shapes"]=json!((0..21).map(|i|json!({"polygon":{"id":format!("p{i}"),"center":"hc","vertex":"hv","sides":1000}})).collect::<Vec<_>>());
        assert!(ids(&value).is_err());
    }
}
