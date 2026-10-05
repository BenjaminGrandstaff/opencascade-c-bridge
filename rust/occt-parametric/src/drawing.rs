//! Regenerating engineering drawings with explicit views and datum annotations.
use super::*;
use crate::assembly::{cross, dot, subtract};
use std::collections::BTreeSet;

mod detail;
mod dimensions;
pub use dimensions::{DimensionPresentation, DimensionTolerance};
mod export;
pub(crate) mod gdt;
mod guides;
pub use gdt::{
    ControlInspection, ControlMeasurement, ControlResult, InspectionRecord, InspectionReport,
    MAX_INSPECTION_POINTS, MeasuredDatumFrame, MeasuredFeature,
};
pub use gdt::{
    DatumMaterialBoundary, DatumPrecedence, DrawingCompositeRefinement,
    DrawingDatumCoordinateFrame, DrawingDatumFeature, DrawingDatumReference,
    DrawingDatumReferenceFrame, DrawingFeatureControlFrame, DrawingGdtAttachment,
    DrawingSizeLimits, FeatureOfSizeKind, GeometricCharacteristic, GeometricToleranceAllowance,
    GeometricToleranceZone, PositionSampleEvaluation, PositionToleranceAxis,
    ResolvedDrawingDatumReference, ResolvedDrawingDatumReferenceFrame, ToleranceMaterialCondition,
};
mod hatching;
pub use guides::{DrawingGuide, DrawingGuideKind, DrawingGuideLine, DrawingGuideLineKind};
pub use hatching::SectionHatching;
mod sheets;
mod slice;
pub use sheets::{DrawingSheet, DrawingSheetOrientation, DrawingSheetSize, ProjectionConvention};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingViewKind {
    #[default]
    Orthographic,
    /// Intersects solids with the view plane and draws only the cut boundaries.
    Slice,
    /// Retains one side of an infinite cutting plane, then removes hidden lines.
    Section {
        origin: VectorQuantity,
        normal: VectorQuantity,
        keep_positive: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawingDetail {
    /// Crop window in view-local model mm. Its minimum maps to paper_origin_mm.
    pub minimum_mm: [f64; 2],
    pub maximum_mm: [f64; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawingView {
    pub id: String,
    pub outputs: Vec<InstanceOutputRef>,
    pub origin: VectorQuantity,
    /// Toward the viewer; x_axis is toward image right.
    pub direction: VectorQuantity,
    pub x_axis: VectorQuantity,
    pub paper_origin_mm: [f64; 2],
    pub scale: f64,
    pub show_hidden: bool,
    #[serde(default)]
    pub kind: DrawingViewKind,
    #[serde(default)]
    pub detail: Option<DrawingDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hatching: Option<SectionHatching>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionDirection {
    Aligned,
    Horizontal,
    Vertical,
    /// First datum is center, second is a point on the circle, in the view plane.
    Radius,
    Diameter,
    /// First and second datums define rays from this vertex; minor angle 0–180°.
    Angular {
        vertex: DatumRef,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawingDimension {
    pub id: String,
    pub view: String,
    pub first: DatumRef,
    pub second: DatumRef,
    pub direction: DimensionDirection,
    /// Signed offset in paper mm, along the dimension's left-hand normal.
    pub offset_mm: f64,
    pub precision: u8,
    #[serde(default, skip_serializing_if = "DimensionPresentation::is_default")]
    pub presentation: DimensionPresentation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingText {
    Literal(String),
    Parameter {
        instance: String,
        parameter: String,
        prefix: String,
        suffix: String,
        precision: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawingNote {
    pub id: String,
    pub position_mm: [f64; 2],
    pub text: DrawingText,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawingDefinition {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datum_reference_frames: Vec<DrawingDatumReferenceFrame>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datum_features: Vec<DrawingDatumFeature>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feature_control_frames: Vec<DrawingFeatureControlFrame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<DrawingSheet>,
    pub id: String,
    pub title: String,
    pub paper_size_mm: [f64; 2],
    pub views: Vec<DrawingView>,
    #[serde(default)]
    pub dimensions: Vec<DrawingDimension>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<DrawingGuide>,
    #[serde(default)]
    pub notes: Vec<DrawingNote>,
    /// Title-block fields, in deterministic key order.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DrawingRenderOptions {
    pub curve_samples: usize,
    pub maximum_vertices: usize,
}
impl Default for DrawingRenderOptions {
    fn default() -> Self {
        Self {
            curve_samples: 64,
            maximum_vertices: 1_000_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawingPolyline {
    pub points_mm: Vec<[f64; 2]>,
    pub hidden: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingDimensionStack {
    pub prefix: String,
    pub upper: String,
    pub lower: String,
    pub suffix: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawingLabel {
    pub position_mm: [f64; 2],
    pub text: String,
    pub stack: Option<DrawingDimensionStack>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedDrawing {
    pub gdt_lines: Vec<DrawingPolyline>,
    pub gdt_labels: Vec<DrawingLabel>,
    pub sheet_lines: Vec<DrawingPolyline>,
    pub sheet_labels: Vec<DrawingLabel>,
    pub id: String,
    pub title: String,
    pub paper_size_mm: [f64; 2],
    pub polylines: Vec<DrawingPolyline>,
    pub guides: Vec<DrawingGuideLine>,
    pub hatches: Vec<DrawingPolyline>,
    pub labels: Vec<DrawingLabel>,
    pub metadata: BTreeMap<String, String>,
    pub generated_variants: usize,
}

fn finite_pair(value: [f64; 2]) -> bool {
    value.iter().all(|value| value.is_finite())
}

fn validate_text(text: &str) -> Result<(), ModelError> {
    if text.len() > 2049 {
        return Err(ModelError::new(
            "drawing text exceeds the 2049-byte DXF field limit",
        ));
    }
    Ok(())
}

fn axis(value: VectorQuantity) -> Result<Vec3, ModelError> {
    let value = value.normalized(Dimension::Scalar)?;
    let maximum = value.x.abs().max(value.y.abs()).max(value.z.abs());
    if maximum == 0.0 || !maximum.is_finite() {
        return Err(ModelError::new("drawing axis must be finite and nonzero"));
    }
    let value = Vec3::new(value.x / maximum, value.y / maximum, value.z / maximum);
    let norm = value.x.hypot(value.y.hypot(value.z));
    Ok(Vec3::new(value.x / norm, value.y / norm, value.z / norm))
}

impl DrawingView {
    fn frame(&self) -> Result<occt_bridge::ProjectionFrame, ModelError> {
        let origin = self.origin.normalized(Dimension::Length)?;
        let direction = axis(self.direction)?;
        let x_axis = axis(self.x_axis)?;
        if ![origin.x, origin.y, origin.z]
            .iter()
            .all(|value| value.is_finite())
            || dot(direction, x_axis).abs() > 1e-9
            || !self.scale.is_finite()
            || self.scale <= 0.0
            || !finite_pair(self.paper_origin_mm)
        {
            return Err(ModelError::new(
                "drawing view requires finite origin/placement, positive scale, and perpendicular axes",
            ));
        }
        let projection = dot(direction, x_axis);
        let x_axis = axis(VectorQuantity::scalars(
            x_axis.x - direction.x * projection,
            x_axis.y - direction.y * projection,
            x_axis.z - direction.z * projection,
        ))?;
        self.validate_section()?;
        hatching::validate(self)?;
        if let Some(detail) = self.detail {
            detail.validate()?;
        }
        Ok(occt_bridge::ProjectionFrame {
            origin,
            direction,
            x_axis,
        })
    }

    fn project(&self, point: Vec3) -> Result<[f64; 2], ModelError> {
        let frame = self.frame()?;
        let delta = subtract(point, frame.origin);
        let local = [
            dot(delta, frame.x_axis),
            dot(delta, cross(frame.direction, frame.x_axis)),
        ];
        if !finite_pair(local) {
            return Err(ModelError::new(
                "drawing projection exceeds finite coordinate limits",
            ));
        }
        Ok(local)
    }

    fn paper(&self, local: [f64; 2]) -> Result<[f64; 2], ModelError> {
        let minimum = self.detail.map_or([0.0, 0.0], |detail| detail.minimum_mm);
        let point = [
            self.paper_origin_mm[0] + self.scale * (local[0] - minimum[0]),
            self.paper_origin_mm[1] + self.scale * (local[1] - minimum[1]),
        ];
        if !finite_pair(point) {
            return Err(ModelError::new(
                "drawing paper coordinates exceed finite limits",
            ));
        }
        Ok(point)
    }

    fn validate_section(&self) -> Result<(), ModelError> {
        if let DrawingViewKind::Section { origin, normal, .. } = &self.kind {
            let origin = origin.normalized(Dimension::Length)?;
            if ![origin.x, origin.y, origin.z]
                .iter()
                .all(|value| value.is_finite())
            {
                return Err(ModelError::new("section plane origin must be finite"));
            }
            axis(*normal)?;
        }
        Ok(())
    }
}

impl DrawingText {
    fn resolve(&self, graph: &InstanceGraph<'_>) -> Result<String, ModelError> {
        match self {
            Self::Literal(text) => {
                validate_text(text)?;
                Ok(text.clone())
            }
            Self::Parameter {
                instance,
                parameter,
                prefix,
                suffix,
                precision,
            } => {
                if *precision > 12 {
                    return Err(ModelError::new("drawing precision must be 0–12"));
                }
                let instance = graph.resolve(instance)?;
                let values = resolve_parameters(instance.definition, &instance.overrides)?;
                let value = values.get(parameter).ok_or_else(|| {
                    ModelError::new(format!("unknown drawing parameter '{parameter}'"))
                })?;
                let value = match value {
                    ParameterValue::Scalar(value) => {
                        format!("{:.*}", usize::from(*precision), value.normalized()?)
                    }
                    ParameterValue::Integer(value) => value.to_string(),
                    ParameterValue::Boolean(value) => value.to_string(),
                    ParameterValue::Choice(value) => value.clone(),
                    ParameterValue::Vector(_) => {
                        return Err(ModelError::new(
                            "drawing parameter notes need a scalar, integer, boolean, or choice",
                        ));
                    }
                };
                let text = format!("{prefix}{value}{suffix}");
                validate_text(&text)?;
                Ok(text)
            }
        }
    }
}

fn datum_origin(graph: &InstanceGraph<'_>, reference: &DatumRef) -> Result<Vec3, ModelError> {
    Ok(match graph.datum(&reference.instance, &reference.datum)? {
        ResolvedDatum::Point { origin }
        | ResolvedDatum::Axis { origin, .. }
        | ResolvedDatum::Plane { origin, .. } => origin,
    })
}

impl DrawingDefinition {
    /// A saved preset controls paper dimensions; otherwise use custom paper_size_mm.
    pub fn effective_paper_size_mm(&self) -> [f64; 2] {
        self.sheet
            .as_ref()
            .map_or(self.paper_size_mm, DrawingSheet::paper_size_mm)
    }

    pub(crate) fn validate(&self, graph: &InstanceGraph<'_>) -> Result<(), ModelError> {
        self.validate_cached(graph, &mut HashMap::new(), &mut HashMap::new())
    }

    fn validate_cached<'definition>(
        &self,
        graph: &InstanceGraph<'definition>,
        resolutions: &mut ResolutionCache<'definition>,
        features: &mut HashMap<&'definition str, HashSet<&'definition str>>,
    ) -> Result<(), ModelError> {
        let paper_size = self.effective_paper_size_mm();
        if self.id.is_empty()
            || !finite_pair(paper_size)
            || paper_size.iter().any(|value| *value <= 0.0)
            || self.views.is_empty()
        {
            return Err(ModelError::new(
                "drawing needs an ID, positive finite paper size, and views",
            ));
        }
        validate_text(&self.title)?;
        if let Some(sheet) = &self.sheet {
            sheet.validate(self)?;
        }
        for (key, value) in &self.metadata {
            if key.len().saturating_add(value.len()).saturating_add(2) > 2049 {
                return Err(ModelError::new(
                    "drawing title-block field exceeds the DXF text limit",
                ));
            }
        }
        let mut ids = HashSet::new();
        for view in &self.views {
            if view.id.is_empty() || !ids.insert(&view.id) || view.outputs.is_empty() {
                return Err(ModelError::new("drawing views need unique IDs and outputs"));
            }
            view.frame()?;
            validate_outputs(view, graph, resolutions, features)?;
        }
        let views = self
            .views
            .iter()
            .map(|view| (view.id.as_str(), view))
            .collect::<HashMap<_, _>>();
        validate_dimensions(&self.dimensions, &views, graph)?;
        guides::validate(&self.guides, &views, graph)?;
        gdt::validate(
            &self.datum_features,
            &self.feature_control_frames,
            &self.datum_reference_frames,
            &views,
            graph,
        )?;
        let mut ids = HashSet::new();
        for note in &self.notes {
            if note.id.is_empty() || !ids.insert(&note.id) || !finite_pair(note.position_mm) {
                return Err(ModelError::new(
                    "drawing notes need unique IDs and finite positions",
                ));
            }
            note.text.resolve(graph)?;
        }
        Ok(())
    }

    /// Regenerates each participating parameter variant once, at current poses.
    /// Exact HLR determines visibility; exported curves are bounded uniform-
    /// parameter polyline approximations, not chordal-error certified geometry.
    /// Time: generation + kernel edge/face HLR work + O(exported vertices).
    /// Storage: generated outputs + projection topology + exported vertices.
    pub fn generate(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        options: DrawingRenderOptions,
    ) -> Result<GeneratedDrawing, ModelError> {
        let mut drawings =
            Self::generate_many(std::slice::from_ref(self), graph, session, options)?;
        Ok(drawings.pop().expect("one drawing requested"))
    }

    /// Batch generation shares variants across every drawing and bounds total
    /// exported vertices across the batch, including each page's annotations.
    pub fn generate_many(
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
        for definition in definitions {
            if !ids.insert(&definition.id) {
                return Err(ModelError::new("drawing batch IDs must be unique"));
            }
            definition.validate_cached(graph, &mut resolutions, &mut features)?;
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
        let generation = graph.regenerate_instances_current(session, &instances)?;
        let mut vertices = 0;
        definitions
            .iter()
            .map(|definition| {
                definition.generate_from(graph, session, &generation, options, &mut vertices)
            })
            .collect()
    }

    fn generate_from(
        &self,
        graph: &InstanceGraph<'_>,
        session: &Session,
        generation: &GraphRegeneration<'_>,
        options: DrawingRenderOptions,
        vertices: &mut usize,
    ) -> Result<GeneratedDrawing, ModelError> {
        let mut drawing = GeneratedDrawing {
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
            .ok_or_else(|| ModelError::new("GD&T vertex count overflow"))?;
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
        *vertices = vertices
            .checked_add(added)
            .ok_or_else(|| ModelError::new("drawing vertex count overflow"))?;
        if *vertices > options.maximum_vertices {
            return Err(ModelError::new("drawing exceeds export vertex budget"));
        }
        for view in &self.views {
            append_view(session, view, generation, options, vertices, &mut drawing)?;
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
        guides::append(&self.guides, &views, graph, &mut drawing)?;
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
            });
        }
        Ok(drawing)
    }
}

fn validate_outputs<'definition>(
    view: &DrawingView,
    graph: &InstanceGraph<'definition>,
    resolutions: &mut ResolutionCache<'definition>,
    features: &mut HashMap<&'definition str, HashSet<&'definition str>>,
) -> Result<(), ModelError> {
    let mut seen = HashSet::new();
    for output in &view.outputs {
        if !seen.insert(&output.instance) || graph.is_suppressed(&output.instance) {
            return Err(ModelError::new(
                "drawing views need distinct unsuppressed instances",
            ));
        }
        let instance = graph.resolve_cached(&output.instance, resolutions)?;
        let names = features
            .entry(instance.definition.id.as_str())
            .or_insert_with(|| {
                instance
                    .definition
                    .features
                    .iter()
                    .map(|feature| feature.id.as_str())
                    .collect()
            });
        if !names.contains(output.output.as_str()) {
            return Err(ModelError::new(format!(
                "unknown drawing output '{}:{}'",
                output.instance, output.output
            )));
        }
    }
    Ok(())
}

fn validate_dimensions(
    dimensions: &[DrawingDimension],
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    let mut ids = HashSet::new();
    let context = dimensions::DimensionContext::new(dimensions, graph)?;
    for dimension in dimensions {
        if dimension.id.is_empty()
            || !ids.insert(&dimension.id)
            || !dimension.offset_mm.is_finite()
            || dimension.precision > 12
        {
            return Err(ModelError::new(
                "drawing dimensions need unique IDs, finite offsets, and precision 0–12",
            ));
        }
        let view = views
            .get(dimension.view.as_str())
            .ok_or_else(|| ModelError::new("drawing dimension references an unknown view"))?;
        dimensions::validate(dimension, view, graph, &context)?;
    }
    Ok(())
}

fn append_view(
    session: &Session,
    view: &DrawingView,
    generation: &GraphRegeneration<'_>,
    options: DrawingRenderOptions,
    vertices: &mut usize,
    drawing: &mut GeneratedDrawing,
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
    if view.hatching.is_some() && matches!(view.kind, DrawingViewKind::Section { .. }) {
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
            drawing
                .polylines
                .push(DrawingPolyline { points_mm, hidden });
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

type DimensionGeometry = ([f64; 2], [f64; 2], [f64; 2], f64);

fn dimension_geometry(
    dimension: &DrawingDimension,
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
) -> Result<DimensionGeometry, ModelError> {
    let first = view.project(datum_origin(graph, &dimension.first)?)?;
    let second = view.project(datum_origin(graph, &dimension.second)?)?;
    let delta = [second[0] - first[0], second[1] - first[1]];
    let (direction, value) = match &dimension.direction {
        DimensionDirection::Aligned => {
            let value = delta[0].hypot(delta[1]);
            ([delta[0] / value, delta[1] / value], value)
        }
        DimensionDirection::Horizontal => ([1.0, 0.0], delta[0].abs()),
        DimensionDirection::Vertical => ([0.0, 1.0], delta[1].abs()),
        DimensionDirection::Radius | DimensionDirection::Diameter => {
            dimensions::require_coplanar(view, graph, &[&dimension.first, &dimension.second])?;
            let radius = delta[0].hypot(delta[1]);
            let value = if matches!(dimension.direction, DimensionDirection::Diameter) {
                2.0 * radius
            } else {
                radius
            };
            ([delta[0] / radius, delta[1] / radius], value)
        }
        DimensionDirection::Angular { .. } => {
            return dimensions::angular_geometry(dimension, view, graph);
        }
    };
    if !value.is_finite() || value <= 1e-12 {
        return Err(ModelError::new(
            "drawing dimension has no finite projected extent",
        ));
    }
    Ok((view.paper(first)?, view.paper(second)?, direction, value))
}

fn append_dimension(
    dimension: &DrawingDimension,
    view: &DrawingView,
    graph: &InstanceGraph<'_>,
    drawing: &mut GeneratedDrawing,
    context: &dimensions::DimensionContext<'_>,
) -> Result<(), ModelError> {
    if matches!(
        dimension.direction,
        DimensionDirection::Angular { .. }
            | DimensionDirection::Radius
            | DimensionDirection::Diameter
    ) {
        return dimensions::append_special(dimension, view, graph, drawing, context);
    }
    let (first, second, direction, value) = dimension_geometry(dimension, view, graph)?;
    let normal = [-direction[1], direction[0]];
    let first_end = [
        first[0] + normal[0] * dimension.offset_mm,
        first[1] + normal[1] * dimension.offset_mm,
    ];
    let mut second_end = [
        second[0] + normal[0] * dimension.offset_mm,
        second[1] + normal[1] * dimension.offset_mm,
    ];
    match &dimension.direction {
        DimensionDirection::Horizontal => second_end[1] = first_end[1],
        DimensionDirection::Vertical => second_end[0] = first_end[0],
        _ => {}
    }
    for points in [
        [first, first_end],
        [second, second_end],
        [first_end, second_end],
    ] {
        if !points.iter().all(|point| finite_pair(*point)) {
            return Err(ModelError::new(
                "drawing dimension exceeds finite coordinates",
            ));
        }
        drawing.polylines.push(DrawingPolyline {
            points_mm: points.to_vec(),
            hidden: false,
        });
    }
    append_arrows(first_end, second_end, drawing)?;
    drawing.labels.push(DrawingLabel {
        position_mm: [
            0.5 * first_end[0] + 0.5 * second_end[0] + normal[0] * 2.0,
            0.5 * first_end[1] + 0.5 * second_end[1] + normal[1] * 2.0,
        ],
        text: dimensions::label(dimension, value, context)?,
        stack: None,
    });
    dimensions::stack_label(dimension, drawing);
    dimensions::decorate_basic(dimension, drawing)?;
    Ok(())
}

fn append_arrows(
    first: [f64; 2],
    second: [f64; 2],
    drawing: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let delta = [second[0] - first[0], second[1] - first[1]];
    let length = delta[0].hypot(delta[1]);
    if !length.is_finite() || length <= 0.0 {
        return Err(ModelError::new(
            "drawing dimension has no finite paper extent",
        ));
    }
    let along = [delta[0] / length, delta[1] / length];
    for (tip, sign) in [(first, 1.0), (second, -1.0)] {
        for side in [-1.0, 1.0] {
            let wing = [
                tip[0] + sign * 2.0 * along[0] - side * 0.7 * along[1],
                tip[1] + sign * 2.0 * along[1] + side * 0.7 * along[0],
            ];
            if !finite_pair(wing) {
                return Err(ModelError::new("drawing arrow exceeds finite coordinates"));
            }
            drawing.polylines.push(DrawingPolyline {
                points_mm: vec![tip, wing],
                hidden: false,
            });
        }
    }
    Ok(())
}
