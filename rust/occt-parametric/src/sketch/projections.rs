//! Structural expansion of declared projections; seeds validate IDs/types only.
//! O(sketch definition size + projections) cloning/storage; never used as solved data.
use super::*;
impl SketchDefinition {
    pub(crate) fn projection_seed(&self) -> Result<Self, ModelError> {
        if self.projections.len() > 1000 {
            return Err(ModelError::new("sketch projections exceed 1000"));
        }
        let mut resolved = self.clone();
        resolved.projections.clear();
        let mut junction_count = 0usize;
        let mut intersection_count = 0usize;
        let kinds: HashMap<_, _> = self
            .projections
            .iter()
            .map(|p| (p.id.as_str(), p.kind))
            .collect();
        for projection in &self.projections {
            intersection_count += projection.intersections.len();
            if projection.intersections.len() > 1000 || intersection_count > 10000 {
                return Err(ModelError::new(
                    "intersection references exceed 1000 per projection or 10000 per sketch",
                ));
            }
            let mut intersection_names = HashSet::new();
            for reference in &projection.intersections {
                if reference.id.is_empty() || !intersection_names.insert(&reference.id) {
                    return Err(ModelError::new(
                        "intersection reference IDs must be nonempty and unique",
                    ));
                }
                if !kinds.contains_key(reference.other.as_str()) {
                    return Err(ModelError::new(
                        "intersection requires another declared projection",
                    ));
                }
                if reference.other == projection.id {
                    return Err(ModelError::new(
                        "intersection requires distinct projections",
                    ));
                }
            }
            junction_count += projection.junctions.len();
            if projection.junctions.len() > 1000 || junction_count > 10000 {
                return Err(ModelError::new(
                    "junction references exceed 1000 per projection or 10000 per sketch",
                ));
            }
            if !projection.junctions.is_empty() && projection.kind != SketchProjectionKind::Chain {
                return Err(ModelError::new(
                    "junction references require a chain projection",
                ));
            }
            let mut names = HashSet::new();
            for junction in &projection.junctions {
                if junction.id.is_empty() || !names.insert(&junction.id) {
                    return Err(ModelError::new(
                        "junction reference IDs must be nonempty and unique",
                    ));
                }
            }
            if projection.id.is_empty() || projection.input.is_empty() {
                return Err(ModelError::new(
                    "sketch projections require nonempty IDs and source features",
                ));
            }
            let id = &projection.id;
            let mut point = |suffix: &str, x: f64, y: f64| {
                let name = format!("{id}:{suffix}");
                resolved.points.push(SketchPoint {
                    id: name.clone(),
                    x: ScalarExpr::Literal(Quantity::length(x, LengthUnit::Millimeter)),
                    y: ScalarExpr::Literal(Quantity::length(y, LengthUnit::Millimeter)),
                    fixed: true,
                });
                name
            };
            for reference in &projection.intersections {
                point(&format!("intersection:{}", reference.id), 0., 0.);
            }
            for junction in &projection.junctions {
                point(&format!("junction:{}", junction.id), 0., 0.);
            }
            match projection.kind {
                SketchProjectionKind::Spline | SketchProjectionKind::Chain => {
                    resolved.splines.push(SketchSpline {
                        id: id.clone(),
                        points: vec![point("start", 0., 0.), point("end", 1., 0.)],
                        basis: Some(SketchBSplineBasis {
                            degree: 1,
                            knots: vec![0., 1.],
                            multiplicities: vec![2, 2],
                            weights: Vec::new(),
                            periodic: false,
                            non_clamped: false,
                        }),
                        endpoints: None,
                    })
                }
                SketchProjectionKind::Line => resolved.lines.push(SketchLine {
                    id: id.clone(),
                    start: point("start", 0., 0.),
                    end: point("end", 1., 0.),
                }),
                SketchProjectionKind::Circle => resolved.circles.push(SketchCircle {
                    id: id.clone(),
                    center: point("center", 0., 0.),
                    rim: point("rim", 1., 0.),
                }),
                SketchProjectionKind::Arc => resolved.arcs.push(SketchArc {
                    id: id.clone(),
                    center: point("center", 0., 0.),
                    start: point("start", 1., 0.),
                    end: point("end", 0., 1.),
                    clockwise: false,
                }),
                SketchProjectionKind::Ellipse => resolved.ellipses.push(SketchEllipse {
                    start: None,
                    end: None,
                    clockwise: false,
                    id: id.clone(),
                    center: point("center", 0., 0.),
                    major: point("major", 2., 0.),
                    minor: point("minor", 0., 1.),
                }),
            }
        }
        Ok(resolved)
    }
}
