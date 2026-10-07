//! Explicit material-ID pattern overrides, with inherited assignment caching.
use super::*;

pub(crate) fn validate_materials(
    view: &DrawingView,
    known: &HashSet<&str>,
) -> Result<(), ModelError> {
    if view.material_hatching.len() > 10_000 {
        return Err(ModelError::new(
            "material hatching allows at most 10000 materials",
        ));
    }
    for (material, patterns) in &view.material_hatching {
        if !known.contains(material.as_str()) {
            return Err(ModelError::new(format!(
                "drawing hatching references unknown material '{material}'"
            )));
        }
        if patterns.len() > 8 {
            return Err(ModelError::new(
                "material hatching allows at most 8 line families per material",
            ));
        }
        for pattern in patterns {
            validate_pattern(*pattern)?;
        }
    }
    Ok(())
}

pub(crate) struct MaterialCache<'graph, 'definition> {
    graph: &'graph InstanceGraph<'definition>,
    inherited: HashMap<String, Option<String>>,
    known: HashSet<&'graph str>,
}
impl<'graph, 'definition> MaterialCache<'graph, 'definition> {
    pub(crate) fn new(graph: &'graph InstanceGraph<'definition>) -> Self {
        Self {
            graph,
            inherited: HashMap::new(),
            known: graph
                .assembly
                .materials
                .iter()
                .map(|m| m.id.as_str())
                .collect(),
        }
    }
    fn selected(
        &mut self,
        view: &DrawingView,
        instance: &str,
    ) -> Result<Option<String>, ModelError> {
        // Output instances/clone chains were validated before regeneration.
        let material =
            crate::change_impact::inherited_material(instance, self.graph, &mut self.inherited);
        if let Some(id) = &material
            && !self.known.contains(id.as_str())
        {
            return Err(ModelError::new(format!(
                "instance '{instance}' has unknown material '{id}'"
            )));
        }
        Ok(material.filter(|id| view.material_hatching.contains_key(id)))
    }
}

pub(crate) struct SectionContext<'session, 'output> {
    pub(crate) session: &'session Session,
    pub(crate) options: DrawingRenderOptions,
    pub(crate) vertices: &'output mut usize,
    pub(crate) drawing: &'output mut GeneratedDrawing,
}
impl<'session> SectionContext<'session, '_> {
    /// O(M + N) cached material ancestry across the batch, plus O(N log G)
    /// grouping per view for N outputs and G selected materials. Each solid is
    /// sectioned once; line families reuse those faces and share one work budget.
    /// Sections/handles occupy O(N + G), in addition to kernel topology and hatches.
    pub(crate) fn append(
        &mut self,
        view: &DrawingView,
        shapes: &[&Shape<'_>],
        materials: &mut MaterialCache<'_, '_>,
    ) -> Result<Shape<'session>, ModelError> {
        let mut groups: BTreeMap<Option<String>, Vec<&Shape<'_>>> = BTreeMap::new();
        for (output, shape) in view.outputs.iter().zip(shapes) {
            groups
                .entry(materials.selected(view, &output.instance)?)
                .or_default()
                .push(shape);
        }
        let mut plane = match view.kind {
            DrawingViewKind::Section { .. } => section_plane_view(view)?,
            DrawingViewKind::Slice => view.clone(),
            DrawingViewKind::Orthographic => {
                return Err(ModelError::new(
                    "material hatching requires a Slice or Section view",
                ));
            }
        };
        plane.hatching = None;
        plane.material_hatching.clear();
        let mut pattern_view = view.clone();
        pattern_view.material_hatching.clear();
        let mut sections = Vec::with_capacity(groups.len());
        let mut work = 0;
        for (material, shapes) in groups {
            let combined = self.session.create_compound(&shapes)?;
            let section = slice::intersection(self.session, &plane, &combined)?;
            let patterns = match &material {
                Some(material) => view.material_hatching[material].clone(),
                None => view.hatching.into_iter().collect(),
            };
            for pattern in patterns {
                pattern_view.hatching = Some(pattern);
                append_with_work(
                    self.session,
                    &pattern_view,
                    &section,
                    self.options,
                    self.vertices,
                    self.drawing,
                    &mut work,
                )?;
            }
            sections.push(section);
        }
        Ok(self
            .session
            .create_compound(&sections.iter().collect::<Vec<_>>())?)
    }
}
