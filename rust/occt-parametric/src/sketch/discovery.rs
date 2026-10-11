//! Native arrangement construction, shared by face outputs and previews.
use super::*;
pub(crate) trait SketchDefinitionDiscoveryInternal {
    fn discovered_regions<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        discovery: &SketchRegionDiscovery,
        frame: (Vec3, Vec3, Vec3),
    ) -> Result<Shape<'a>, ModelError>;
}

impl SketchDefinitionDiscoveryInternal for SketchDefinition {
    fn discovered_regions<'a>(
        &self,
        session: &'a Session,
        parameters: &HashMap<String, ParameterValue>,
        solution: &SketchSolution,
        discovery: &SketchRegionDiscovery,
        frame: (Vec3, Vec3, Vec3),
    ) -> Result<Shape<'a>, ModelError> {
        let (origin, x_axis, y_axis) = frame;
        let transform = |p: SketchPoint2| add(origin, add(scale(x_axis, p.x), scale(y_axis, p.y)));
        let normal = cross(x_axis, y_axis);
        let library = self.curve_library(session, parameters, solution, frame)?;
        let ids = if discovery.entities.is_empty() {
            &library.active
        } else {
            &discovery.entities
        };
        let wires: Vec<_> = ids.iter().map(|id| &library.curves[id]).collect();
        let seeds: Vec<_> = discovery
            .seeds
            .iter()
            .map(|id| transform(solution.points[id]))
            .collect();
        let cells = session
            .discover_planar_regions(&wires, origin, normal, x_axis, &seeds)
            .map_err(|e| {
                ModelError::from(e).context(&format!("sketch '{}' region discovery", self.id))
            })?;
        if !discovery.merge_adjacent {
            return Ok(cells);
        }
        let area = session.surface_area(&cells)?;
        let regions = session.unify_same_domain(&cells, 1e-7, 1e-9).map_err(|e| {
            ModelError::from(e).context(&format!("sketch '{}' region composition", self.id))
        })?;
        if !session.is_valid(&regions)?
            || (session.surface_area(&regions)? - area).abs() > area * 1e-8 + 1e-9
        {
            return Err(ModelError::new(format!(
                "sketch '{}' region composition must preserve valid material area",
                self.id
            )));
        }
        Ok(regions)
    }
}
