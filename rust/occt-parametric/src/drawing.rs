//! Regenerating engineering drawings with explicit views and datum annotations.
use super::*;
use crate::assembly::{cross, dot};
#[allow(unused_imports)]
pub use occt_parametric_core::drawing::*;
use std::collections::BTreeSet;
mod curves;
mod hatching;
mod nominals;
mod pmi;
mod texture_faces;
pub use nominals::DrawingDefinitionInspection;
pub use pmi::DrawingStepPmi;
pub use texture_faces::{DrawingSurfaceTextureFaces, SurfaceTextureFaces};

mod slice;
pub trait DrawingDefinitionRendering {
    /// Regenerates each participating parameter variant once, at current poses.
    /// Exact HLR determines visibility. Exact mode preserves standard curves;
    /// default mode exports uniform-parameter polyline approximations.
    /// Time: generation + kernel edge/face HLR work + O(exported vertices).
    /// Storage: generated outputs + projection topology + exported vertices.
    fn generate(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        options: DrawingRenderOptions,
    ) -> Result<GeneratedDrawing, ModelError>;

    /// Batch generation shares variants across every drawing and bounds total
    /// exported vertices across the batch, including each page's annotations.
    fn generate_many(
        definitions: &[Self],
        graph: &InstanceGraph<'_>,
        session: &Session,
        options: DrawingRenderOptions,
    ) -> Result<Vec<GeneratedDrawing>, ModelError>
    where
        Self: Sized;
}

impl DrawingDefinitionRendering for DrawingDefinition {
    fn generate(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        options: DrawingRenderOptions,
    ) -> Result<GeneratedDrawing, ModelError> {
        let mut drawings =
            Self::generate_many(std::slice::from_ref(self), graph, session, options)?;
        Ok(drawings.pop().expect("one drawing requested"))
    }

    fn generate_many(
        definitions: &[Self],
        graph: &InstanceGraph<'_>,
        session: &Session,
        options: DrawingRenderOptions,
    ) -> Result<Vec<GeneratedDrawing>, ModelError> {
        if !(1..=10_000).contains(&definitions.len()) {
            return Err(ModelError::new("drawing batch needs 1–10000 definitions"));
        }
        let mut ids = HashSet::new();
        let mut resolutions = HashMap::new();
        let mut features = HashMap::new();
        let materials = graph
            .assembly
            .materials
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        for definition in definitions {
            if !ids.insert(&definition.id) {
                return Err(ModelError::new("drawing batch IDs must be unique"));
            }
            definition.validate_cached(graph, &mut resolutions, &mut features, &materials)?;
        }
        if !options.curve_tolerance_mm.is_finite() || options.curve_tolerance_mm <= 0.0 {
            return Err(ModelError::new(
                "drawing curve tolerance must be finite and positive",
            ));
        }
        if !(2..=100_000).contains(&options.curve_samples) || options.maximum_vertices < 2 {
            return Err(ModelError::new(
                "drawing export needs 2–100000 samples and a positive vertex budget",
            ));
        }
        let instances = definitions
            .iter()
            .flat_map(|definition| &definition.views)
            .flat_map(|view| view.outputs.iter().map(|output| output.instance.as_str()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        // Sheet status tables list the revisions of sibling sheets, which
        // must be in the batch; their vertices are budgeted per sheet.
        let statuses = releases::sheet_statuses(definitions)?;
        let generation = graph.regenerate_instances_current(session, &instances)?;
        let mut vertices = 0;
        let mut hatching = hatching::materials::MaterialCache::new(graph);
        definitions
            .iter()
            .map(|definition| {
                let mut drawing = definition.generate_from(
                    graph,
                    session,
                    &generation,
                    options,
                    &mut vertices,
                    &mut hatching,
                )?;
                if let Some(rows) = statuses.get(&definition.id) {
                    releases::append_sheet_status(definition, rows, &mut drawing);
                }
                Ok(drawing)
            })
            .collect()
    }
}

pub(crate) trait DrawingDefinitionRenderingInternal {
    fn generate_from(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        options: DrawingRenderOptions,
        vertices: &mut usize,
        hatching: &mut hatching::materials::MaterialCache<'_, '_>,
    ) -> Result<GeneratedDrawing, ModelError>;
}

impl DrawingDefinitionRenderingInternal for DrawingDefinition {
    fn generate_from(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        options: DrawingRenderOptions,
        vertices: &mut usize,
        hatching: &mut hatching::materials::MaterialCache<'_, '_>,
    ) -> Result<GeneratedDrawing, ModelError> {
        let mut drawing = GeneratedDrawing {
            curves: Vec::new(),
            gdt_lines: Vec::new(),
            gdt_labels: Vec::new(),
            sheet_lines: Vec::new(),
            sheet_labels: Vec::new(),
            id: self.id.clone(),
            title: self.title.clone(),
            paper_size_mm: self.effective_paper_size_mm(),
            polylines: Vec::new(),
            guides: Vec::new(),
            hatches: Vec::new(),
            labels: Vec::new(),
            metadata: self.metadata.clone(),
            generated_variants: generation.generated_variants(),
            lettering: DrawingLettering::for_sheet(self.sheet.as_ref()),
            line_weights: self.sheet.as_ref().and_then(|sheet| sheet.line_weights),
            zones: None,
        };
        if let Some(sheet) = &self.sheet {
            sheets::decorate(
                &mut drawing,
                sheet,
                &self.views.iter().map(|v| v.scale).collect::<Vec<_>>(),
            );
            let count: usize = drawing
                .sheet_lines
                .iter()
                .map(|line| line.points_mm.len())
                .sum();
            *vertices = vertices
                .checked_add(count)
                .ok_or_else(|| ModelError::new("sheet vertex count overflow"))?;
        }
        *vertices = vertices
            .checked_add(gdt::vertex_count(
                &self.datum_features,
                &self.feature_control_frames,
                &self.datum_reference_frames,
            )?)
            .and_then(|sum| {
                gdt::texture::vertex_count(&self.surface_textures)
                    .ok()
                    .and_then(|count| sum.checked_add(count))
            })
            .ok_or_else(|| ModelError::new("GD&T vertex count overflow"))?;
        let parts = parts_list::Layout::new(self, graph)?;
        *vertices = vertices
            .checked_add(parts.vertex_count(self) + releases::vertex_count(self))
            .ok_or_else(|| ModelError::new("parts list vertex count overflow"))?;
        let added = self
            .dimensions
            .iter()
            .try_fold(guides::vertex_count(&self.guides)?, |sum, d| {
                let count = match d.direction {
                    DimensionDirection::Angular { .. } => 77,
                    DimensionDirection::Radius => 8,
                    DimensionDirection::Diameter => 12,
                    _ => 14,
                } + if matches!(d.presentation.tolerance, DimensionTolerance::Basic) {
                    5
                } else {
                    0
                };
                sum.checked_add(count)
            })
            .ok_or_else(|| ModelError::new("drawing annotation vertex count overflow"))?;
        let thread_lines = thread_lines::lines(&self.views, graph)?;
        let added = thread_lines
            .iter()
            .map(|line| line.points_mm.len())
            .try_fold(added, usize::checked_add)
            .ok_or_else(|| ModelError::new("drawing annotation vertex count overflow"))?;
        *vertices = vertices
            .checked_add(added)
            .ok_or_else(|| ModelError::new("drawing vertex count overflow"))?;
        if *vertices > options.maximum_vertices {
            return Err(ModelError::new("drawing exceeds export vertex budget"));
        }
        for view in &self.views {
            append_view(
                session,
                view,
                generation,
                options,
                vertices,
                &mut drawing,
                hatching,
            )?;
        }
        let views = self
            .views
            .iter()
            .map(|view| (view.id.as_str(), view))
            .collect::<HashMap<_, _>>();
        gdt::append(
            &self.datum_features,
            &self.feature_control_frames,
            &self.datum_reference_frames,
            &views,
            graph,
            &mut drawing,
        )?;
        let landings = if self
            .surface_textures
            .iter()
            .any(|t| t.attachment.faces.is_some())
        {
            self.surface_texture_faces(graph, session)?
                .into_iter()
                .map(|faces| {
                    (faces.texture, {
                        let [x, y, z] = faces.landing_mm;
                        Vec3::new(x, y, z)
                    })
                })
                .collect()
        } else {
            HashMap::new()
        };
        gdt::texture::append(
            &self.surface_textures,
            &views,
            graph,
            &landings,
            &mut drawing,
        )?;
        parts.append(self, &views, graph, &mut drawing)?;
        releases::append(self, &mut drawing);
        releases::append_changes(self, &views, graph, &mut drawing)?;
        guides::append(&self.guides, &views, graph, &mut drawing)?;
        drawing.polylines.extend(thread_lines);
        let context = dimensions::DimensionContext::new(&self.dimensions, graph)?;
        for dimension in &self.dimensions {
            append_dimension(
                dimension,
                views[dimension.view.as_str()],
                graph,
                &mut drawing,
                &context,
            )?;
        }
        for note in &self.notes {
            drawing.labels.push(DrawingLabel {
                position_mm: note.position_mm,
                text: note.text.resolve(graph)?,
                stack: None,
                height_mm: drawing.lettering.notes,
            });
        }
        Ok(drawing)
    }
}

fn append_view(
    session: &Session,
    view: &DrawingView,
    generation: &GraphRegeneration<'_>,
    options: DrawingRenderOptions,
    vertices: &mut usize,
    drawing: &mut GeneratedDrawing,
    hatching: &mut hatching::materials::MaterialCache<'_, '_>,
) -> Result<(), ModelError> {
    let shapes = view
        .outputs
        .iter()
        .map(|output| {
            generation
                .result(&output.instance)
                .and_then(|result| result.shape(&output.output))
                .ok_or_else(|| ModelError::new("generated drawing output is missing"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let combined = session.create_compound(&shapes)?;
    if !view.material_hatching.is_empty() {
        let mut section = hatching::materials::SectionContext {
            session,
            options,
            vertices,
            drawing,
        };
        let cut = section.append(view, &shapes, hatching)?;
        if matches!(view.kind, DrawingViewKind::Slice) {
            return append_edges(session, view, &cut, false, options, vertices, drawing);
        }
    }
    if view.material_hatching.is_empty()
        && view.hatching.is_some()
        && matches!(view.kind, DrawingViewKind::Section { .. })
    {
        let section_view = hatching::section_plane_view(view)?;
        let section = slice::intersection(session, &section_view, &combined)?;
        hatching::append(session, view, &section, options, vertices, drawing)?;
    }
    let combined = match view.kind {
        DrawingViewKind::Slice => {
            return slice::append(session, view, &combined, options, vertices, drawing);
        }
        DrawingViewKind::Orthographic => combined,
        DrawingViewKind::Section {
            origin,
            normal,
            keep_positive,
        } => slice::clip_components(
            session,
            &combined,
            origin.normalized(Dimension::Length)?,
            axis(normal)?,
            keep_positive,
        )?,
    };
    if session.subshape_count(&combined, ShapeType::Edge)? == 0 {
        return Ok(());
    }
    let projected = session.orthographic_projection(&combined, view.frame()?)?;
    append_edges(
        session,
        view,
        &projected.visible,
        false,
        options,
        vertices,
        drawing,
    )?;
    if view.show_hidden {
        append_edges(
            session,
            view,
            &projected.hidden,
            true,
            options,
            vertices,
            drawing,
        )?;
    }
    Ok(())
}

fn append_edges(
    session: &Session,
    view: &DrawingView,
    shape: &Shape<'_>,
    hidden: bool,
    options: DrawingRenderOptions,
    vertices: &mut usize,
    drawing: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    if options.exact_curves {
        return curves::append(session, view, shape, hidden, options, vertices, drawing);
    }
    let count = session.subshape_count(shape, ShapeType::Edge)?;
    let sample_budget = options
        .curve_samples
        .checked_mul(if view.detail.is_some() { 2 } else { 1 })
        .ok_or_else(|| ModelError::new("drawing vertex count overflow"))?;
    let added = count
        .checked_mul(sample_budget)
        .and_then(|count| vertices.checked_add(count))
        .ok_or_else(|| ModelError::new("drawing vertex count overflow"))?;
    if added > options.maximum_vertices {
        return Err(ModelError::new("drawing exceeds export vertex budget"));
    }
    *vertices = added;
    for edge in session.subshapes(shape, ShapeType::Edge)? {
        let local = sampled_points(session, view, &edge, options.curve_samples)?;
        let paths = match view.detail {
            Some(detail) => detail::clip_polyline(&local, detail)?,
            None => vec![local],
        };
        for path in paths {
            let points_mm = path
                .into_iter()
                .map(|point| view.paper(point))
                .collect::<Result<Vec<_>, _>>()?;
            drawing.polylines.push(DrawingPolyline {
                points_mm,
                hidden,
                thin: hidden,
            });
        }
    }
    Ok(())
}

fn sampled_points(
    session: &Session,
    view: &DrawingView,
    edge: &Shape<'_>,
    samples: usize,
) -> Result<Vec<[f64; 2]>, ModelError> {
    session
        .edge_sample_points(edge, samples)?
        .iter()
        .map(|point| {
            if matches!(view.kind, DrawingViewKind::Slice) {
                view.project(*point)
            } else {
                Ok([point.x, point.y])
            }
        })
        .collect()
}
