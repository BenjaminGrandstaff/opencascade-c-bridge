//! Native arrangement construction, shared by face outputs and previews.
use super::*;

impl SketchDefinition {
    pub(super) fn discovered_regions<'a>(
        &self,
        session: &'a Session,
        solution: &SketchSolution,
        discovery: &SketchRegionDiscovery,
        frame: (Vec3, Vec3, Vec3),
    ) -> Result<Shape<'a>, ModelError> {
        let (origin, x_axis, y_axis) = frame;
        let transform = |p: SketchPoint2| add(origin, add(scale(x_axis, p.x), scale(y_axis, p.y)));
        let direction = |p: SketchPoint2| add(scale(x_axis, p.x), scale(y_axis, p.y));
        let normal = cross(x_axis, y_axis);
        let index = self.entities();
        let mut ids: Vec<_> = if discovery.entities.is_empty() {
            index.keys().copied().collect()
        } else {
            discovery.entities.iter().map(String::as_str).collect()
        };
        ids.sort_unstable();
        let wires = ids
            .iter()
            .map(|id| {
                self.entity_wire(session, index[id], solution, &transform, &direction, normal)
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let seeds: Vec<_> = discovery
            .seeds
            .iter()
            .map(|id| transform(solution.points[id]))
            .collect();
        let cells = session
            .discover_planar_regions(
                &wires.iter().collect::<Vec<_>>(),
                origin,
                normal,
                x_axis,
                &seeds,
            )
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
