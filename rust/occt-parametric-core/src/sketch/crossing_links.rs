//! Structural checks for selected native crossings, before any kernel work.
use super::*;
impl SketchDefinition {
    pub fn validate_crossing_links(&self) -> Result<(), ModelError> {
        let entities = self.entities();
        let mut targets = HashSet::new();
        let free = self
            .points
            .iter()
            .filter(|p| !p.fixed)
            .map(|p| p.id.as_str())
            .collect::<HashSet<_>>();
        let mut tangent_neighbors: HashMap<&str, Vec<&str>> = HashMap::new();
        for constraint in &self.constraints {
            if let SketchConstraint::Tangent { first, second, .. } = constraint {
                for (s, n) in [(first, second), (second, first)] {
                    if matches!(
                        entities.get(s.as_str()),
                        Some(Entity::Spline(SketchSpline { basis: None, .. }))
                    ) && entities.contains_key(n.as_str())
                    {
                        tangent_neighbors.entry(s).or_default().push(n);
                    }
                }
            }
        }

        let links = self
            .constraints
            .iter()
            .filter_map(|c| c.dependent_point().map(|id| (id, c)))
            .collect::<HashMap<_, _>>();
        for constraint in &self.constraints {
            let SketchConstraint::CrossingPoint {
                point,
                first,
                second,
                ..
            } = constraint
            else {
                continue;
            };
            if !targets.insert(point.as_str()) || links.contains_key(point.as_str()) {
                return Err(ModelError::new(
                    "selected crossing point targets must be unique and independent of directional point links",
                ));
            }
            if !free.contains(point.as_str()) {
                return Err(ModelError::new(
                    "selected crossing target must be an authored non-fixed point",
                ));
            }
            if first == second
                || [first, second]
                    .iter()
                    .any(|id| !entities.contains_key(id.as_str()))
            {
                return Err(ModelError::new(
                    "selected crossing requires two distinct known curves",
                ));
            }
            let refs = |id: &str| -> Vec<&str> {
                match entities[id] {
                    Entity::Line(l) => vec![&l.start, &l.end],
                    Entity::Circle(c) => vec![&c.center, &c.rim],
                    Entity::Arc(a) => vec![&a.center, &a.start, &a.end],
                    Entity::Ellipse(e) => std::iter::once(e.center.as_str())
                        .chain([e.major.as_str(), e.minor.as_str()])
                        .chain(e.arc_ends().into_iter().flat_map(|(a, b)| [a, b]))
                        .collect(),
                    Entity::Spline(s) => s
                        .points
                        .iter()
                        .map(String::as_str)
                        .chain(
                            s.endpoints
                                .iter()
                                .flat_map(|e| [e.start.as_str(), e.end.as_str()]),
                        )
                        .collect(),
                }
            };
            let mut queue = refs(first);
            queue.extend(refs(second));
            // Interpolation endpoint tangents depend on their authored neighbors.
            for source in [first, second] {
                if let Some(neighbors) = tangent_neighbors.get(source.as_str()) {
                    for neighbor in neighbors {
                        queue.extend(refs(neighbor));
                    }
                }
            }
            let mut seen = HashSet::new();
            while let Some(source) = queue.pop() {
                if source == point {
                    return Err(ModelError::new(
                        "selected crossing target cannot define either source curve, directly or through point links",
                    ));
                }
                if !seen.insert(source) {
                    continue;
                }
                if let Some(link) = links.get(source) {
                    queue.extend(self.point_link_sources(link)?);
                }
            }
        }
        Ok(())
    }
}
