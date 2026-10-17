//! Drawing GD&T as semantic product manufacturing information (schema 144):
//! which datums, tolerances and sizes a STEP AP242 export carries on the
//! faces their attachments name, and the report of what read back.
use super::*;
use gdt::composite;
pub mod views;
use views::AnnotationPlane;
pub use views::{
    AnnotationClippingPlane, AnnotationViewAxes, DrawingAnnotationView, PlannedView,
    PmiPresentation, PmiViewLoss, PmiViewReport,
};

/// What a PMI item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PmiItemKind {
    Datum,
    Tolerance,
    Size,
}

/// A part of a PMI item that an exchange did not carry.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PmiProperty {
    /// The whole item: nothing of its kind was read on its faces.
    Item,
    Value,
    /// Diameter or plain zone.
    Zone,
    Material,
    Modifiers,
    ProjectedZone,
    Datums,
    /// The annotation's 3D presentation, polyline for polyline.
    Presentation,
    ContinuousFeature,
    DynamicProfile,
    UnequalZone,
    /// How a composite's lower segment relates to the segments above it; the
    /// segment itself is written as a tolerance of its own.
    CompositeRelation,
    /// SEP REQT. AP242 has a separate-requirement modifier, but the bridge's
    /// STEP writer does not set it yet.
    SeparateRequirement,
}

/// One exported PMI item: `unrepresented` lists what the exchange format
/// cannot hold, `lost` what it should hold but did not read back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PmiExchangeItem {
    /// The datum feature or feature control frame, or the first member of a
    /// pattern.
    pub id: String,
    pub kind: PmiItemKind,
    /// Composite segment, 0 for the top; always 0 for datums and sizes.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub segment: usize,
    pub instance: String,
    pub faces: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unrepresented: Vec<PmiProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lost: Vec<PmiProperty>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// An annotation left out of the exchange, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PmiSkipped {
    pub id: String,
    pub reason: String,
}

/// What a semantic PMI export wrote and what reading it back recovered.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PmiExchangeReport {
    pub parts: usize,
    pub items: Vec<PmiExchangeItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<PmiSkipped>,
    /// Saved annotation views (schema 145).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<PmiViewReport>,
    /// Dataset identification (schema 151).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset: Option<PmiDatasetReport>,
}

impl PmiExchangeReport {
    /// Every exported item and view read back with nothing lost.
    pub fn verified(&self) -> bool {
        self.items.iter().all(|item| item.lost.is_empty())
            && self.views.iter().all(|view| {
                view.lost.is_none()
                    && view.lost_annotations.is_empty()
                    && view.lost_sizes.is_empty()
            })
            && self
                .dataset
                .as_ref()
                .is_none_or(|dataset| dataset.lost.is_empty())
    }

    /// Verified, and nothing was skipped, omitted from a view or left
    /// unrepresented.
    pub fn complete(&self) -> bool {
        self.verified()
            && self.skipped.is_empty()
            && self.items.iter().all(|item| item.unrepresented.is_empty())
            && self.views.iter().all(|view| view.omitted.is_empty())
    }
}

/// Faces of one output: the union of the selectors' faces.
#[derive(Clone, Debug, PartialEq)]
pub struct PmiTarget<'a> {
    pub output: &'a InstanceOutputRef,
    pub faces: Vec<&'a FaceSelector>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedDatum<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub target: PmiTarget<'a>,
    /// In the plane of the first annotation view that shows the feature.
    pub presentation: Option<PmiPresentation>,
}

/// A datum reference: an index into [`PmiPlan::datums`] in compartment 0
/// (primary), 1 or 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannedReference {
    pub datum: usize,
    pub compartment: u32,
    pub boundary: DatumMaterialBoundary,
    pub translation: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedTolerance<'a> {
    pub id: &'a str,
    pub segment: usize,
    pub target: PmiTarget<'a>,
    pub characteristic: GeometricCharacteristic,
    pub value_mm: f64,
    pub diameter: bool,
    pub material: ToleranceMaterialCondition,
    pub free_state: bool,
    pub statistical: bool,
    pub tangent_plane: bool,
    pub projected_mm: Option<f64>,
    pub datums: Vec<PlannedReference>,
    pub unrepresented: Vec<PmiProperty>,
    /// In the plane of the first annotation view that shows the frame or a
    /// pattern member: the whole frame, or a composite segment's own row.
    pub presentation: Option<PmiPresentation>,
}

/// Size limits of a feature of size; its kind (diameter or width) follows
/// from its faces.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedSize<'a> {
    pub id: &'a str,
    pub target: PmiTarget<'a>,
    pub lower_mm: f64,
    pub upper_mm: f64,
    /// Where the size dimension is drawn when an annotation view shows it:
    /// above its frame or datum feature symbol, in that annotation's plane.
    drawing: Option<SizeDrawing>,
}

#[derive(Clone, Debug, PartialEq)]
struct SizeDrawing {
    plane: AnnotationPlane,
    position: [f64; 2],
    /// Members of the pattern whose frame it is drawn above.
    count: usize,
    lettering: DrawingLettering,
}

impl PlannedSize<'_> {
    /// The size dimension `[nX ][Ø]lower-upper` drawn in 3D, as a diameter
    /// or a width as its faces give, when a view shows it.
    pub fn presentation(&self, diameter: bool) -> Result<Option<PmiPresentation>, ModelError> {
        let Some(drawing) = &self.drawing else {
            return Ok(None);
        };
        let mut d = gdt::scratch(drawing.lettering);
        let mut text = String::new();
        if drawing.count > 1 {
            text.push_str(&format!("{}X ", drawing.count));
        }
        if diameter {
            text.push('Ø');
        }
        text.push_str(&limits_text(self.lower_mm, self.upper_mm));
        d.gdt_labels.push(DrawingLabel {
            position_mm: drawing.position,
            text,
            stack: None,
            height_mm: drawing.lettering.notes,
        });
        drawing
            .plane
            .presentation(format!("size {}", self.id), &d)
            .map(Some)
    }

    /// Whether an annotation view shows this size dimension.
    pub fn drawn(&self) -> bool {
        self.drawing.is_some()
    }
}

/// The drawing GD&T a PMI export writes, in drawing order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PmiPlan<'a> {
    pub datums: Vec<PlannedDatum<'a>>,
    pub tolerances: Vec<PlannedTolerance<'a>>,
    pub sizes: Vec<PlannedSize<'a>>,
    pub skipped: Vec<PmiSkipped>,
    pub views: Vec<PlannedView<'a>>,
    pub dataset: Option<PlannedDataset<'a>>,
}

/// The dataset identification to write: the drawing's dataset with its
/// latest release, if any.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedDataset<'a> {
    pub dataset: &'a DrawingDataset,
    pub release: Option<&'a DrawingRelease>,
}

/// What of the dataset identification did not read back.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PmiDatasetProperty {
    Identifier,
    Revision,
    Authority,
    Approvals,
}

/// The exported dataset identification and what reading it back recovered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PmiDatasetReport {
    pub identifier: String,
    pub authority: DatasetAuthority,
    /// The latest release's revision; none while unreleased.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lost: Vec<PmiDatasetProperty>,
}

fn skip(skipped: &mut Vec<PmiSkipped>, id: &str, reason: impl Into<String>) {
    skipped.push(PmiSkipped {
        id: id.to_owned(),
        reason: reason.into(),
    });
}

fn limits_mm(limits: &DrawingSizeLimits) -> Result<(f64, f64), ModelError> {
    Ok((limits.lower.normalized()?, limits.upper.normalized()?))
}

/// Limit dimensioning on one line (Y14.5): the lower limit, a dash and the
/// upper, both in millimetres with the same decimals (at most four).
fn limits_text(lower_mm: f64, upper_mm: f64) -> String {
    let decimals = |value: f64| {
        (0..4)
            .find(|&d| {
                let scale = 10f64.powi(d);
                ((value * scale).round() / scale - value).abs() <= 1e-9 * value.abs().max(1.0)
            })
            .unwrap_or(4) as usize
    };
    let d = decimals(lower_mm).max(decimals(upper_mm));
    format!("{lower_mm:.d$}-{upper_mm:.d$}")
}

impl DrawingDefinition {
    /// The datum features and feature control frames whose attachments name
    /// `faces` on an output for which `exported` holds, as PMI on those
    /// faces. Pattern members on one output become one tolerance over all
    /// their faces; each composite segment becomes a tolerance of its own.
    /// The rest are skipped with a reason: no faces, an output not exported,
    /// or a datum reference that is skipped or lies on another output.
    /// Surface textures are always skipped.
    /// Annotations an annotation view shows are drawn as 3D presentations,
    /// and the views list the planned items they show. O(features + frames ·
    /// segments + view entries + presentation linework).
    pub fn pmi_plan<'a>(
        &'a self,
        graph: &InstanceGraph<'_>,
        exported: impl Fn(&InstanceOutputRef) -> bool,
    ) -> Result<PmiPlan<'a>, ModelError> {
        self.validate(graph)?;
        let mut plan = PmiPlan::default();
        let lettering = DrawingLettering::for_sheet(self.sheet.as_ref());
        // The first view showing each datum feature and frame sets its plane.
        let mut feature_view: HashMap<&str, usize> = HashMap::new();
        let mut frame_view: HashMap<&str, usize> = HashMap::new();
        // IDs whose size dimension some view shows.
        let shown_sizes: HashSet<&str> = self
            .annotation_views
            .iter()
            .flat_map(|view| view.sizes.iter().map(String::as_str))
            .collect();
        for (index, view) in self.annotation_views.iter().enumerate() {
            for id in &view.datum_features {
                feature_view.entry(id.as_str()).or_insert(index);
            }
            for id in &view.feature_control_frames {
                frame_view.entry(id.as_str()).or_insert(index);
            }
        }
        let plane = |view: usize, attachment: &DrawingGdtAttachment| {
            AnnotationPlane::new(
                &self.annotation_views[view],
                datum_origin(graph, &attachment.anchor)?,
            )
        };
        let mut datum_index = HashMap::new();
        // Planned size of each datum feature and frame ID.
        let mut size_index: HashMap<&str, usize> = HashMap::new();
        // Planned tolerances of each frame ID.
        let mut frame_tolerances: HashMap<&str, std::ops::Range<usize>> = HashMap::new();
        for feature in &self.datum_features {
            let attachment = &feature.attachment;
            let Some(faces) = &attachment.faces else {
                skip(&mut plan.skipped, &feature.id, "no persistent faces");
                continue;
            };
            if !exported(&attachment.output) {
                skip(&mut plan.skipped, &feature.id, "output not exported");
                continue;
            }
            let target = PmiTarget {
                output: &attachment.output,
                faces: vec![faces],
            };
            let mut size_drawing = None;
            let presentation = match feature_view.get(feature.id.as_str()) {
                Some(&view) => {
                    let mut d = gdt::scratch(lettering);
                    gdt::draw_datum_feature(feature, [0.0, 0.0], attachment.offset_mm, &mut d);
                    let plane = plane(view, attachment)?;
                    if feature.size_limits.is_some() && shown_sizes.contains(feature.id.as_str()) {
                        // Above the symbol's 8 mm box.
                        let offset = attachment.offset_mm;
                        size_drawing = Some(SizeDrawing {
                            plane: plane.clone(),
                            position: [offset[0], offset[1] + 10.0],
                            count: 1,
                            lettering,
                        });
                    }
                    Some(plane.presentation(format!("datum {}", feature.label), &d)?)
                }
                None => None,
            };
            datum_index.insert(feature.id.as_str(), plan.datums.len());
            plan.datums.push(PlannedDatum {
                id: &feature.id,
                label: &feature.label,
                target: target.clone(),
                presentation,
            });
            if let Some(limits) = &feature.size_limits {
                let (lower_mm, upper_mm) = limits_mm(limits)?;
                size_index.insert(&feature.id, plan.sizes.len());
                plan.sizes.push(PlannedSize {
                    id: &feature.id,
                    target,
                    lower_mm,
                    upper_mm,
                    drawing: size_drawing,
                });
            }
        }
        let resolved =
            gdt::effective_frames(&self.feature_control_frames, &self.datum_reference_frames)?;
        let features: HashMap<_, _> = self
            .datum_features
            .iter()
            .map(|f| (f.id.as_str(), f))
            .collect();
        // Pattern members on one output, in drawing order of their first.
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut grouped: HashMap<(&str, &InstanceOutputRef), usize> = HashMap::new();
        for (index, frame) in self.feature_control_frames.iter().enumerate() {
            match &frame.pattern {
                Some(pattern) => {
                    let key = (pattern.as_str(), &frame.attachment.output);
                    match grouped.get(&key) {
                        Some(&group) => groups[group].push(index),
                        None => {
                            grouped.insert(key, groups.len());
                            groups.push(vec![index]);
                        }
                    }
                }
                None => groups.push(vec![index]),
            }
        }
        for members in groups {
            let frames: Vec<&DrawingFeatureControlFrame> = members
                .iter()
                .map(|&i| &self.feature_control_frames[i])
                .collect();
            let first = frames[0];
            let attachment = &first.attachment;
            let missing = frames.iter().find(|f| f.attachment.faces.is_none());
            let reason = if let Some(member) = missing {
                Some(if frames.len() > 1 {
                    format!("pattern member '{}' has no persistent faces", member.id)
                } else {
                    "no persistent faces".to_owned()
                })
            } else if !exported(&attachment.output) {
                Some("output not exported".to_owned())
            } else {
                None
            };
            if let Some(reason) = reason {
                for frame in &frames {
                    skip(&mut plan.skipped, &frame.id, reason.clone());
                }
                continue;
            }
            let target = PmiTarget {
                output: &attachment.output,
                faces: frames
                    .iter()
                    .filter_map(|f| f.attachment.faces.as_ref())
                    .collect(),
            };
            let segments = composite::segments(&resolved[members[0]]);
            // The pattern's frame is the first member's, drawn with n× above
            // it when the pattern has several.
            let view = frames
                .iter()
                .filter_map(|f| frame_view.get(f.id.as_str()).copied())
                .min();
            let mut presentations: Vec<Option<PmiPresentation>> = vec![None; segments.len()];
            let mut size_drawing = None;
            if let Some(view) = view {
                let mut drawn = vec![gdt::scratch(lettering); segments.len()];
                let offset = attachment.offset_mm;
                gdt::draw_frame(
                    &resolved[members[0]],
                    &features,
                    [0.0, 0.0],
                    offset,
                    &mut drawn,
                )?;
                // A shown size dimension goes above the frame and carries
                // the pattern's count; otherwise the count stands alone.
                let above = [offset[0], offset[1] + 8.0 * segments.len() as f64 + 2.0];
                let plane = plane(view, attachment)?;
                if first.size_limits.is_some()
                    && frames.iter().any(|f| shown_sizes.contains(f.id.as_str()))
                {
                    size_drawing = Some(SizeDrawing {
                        plane: plane.clone(),
                        position: above,
                        count: frames.len(),
                        lettering,
                    });
                } else if frames.len() > 1 {
                    drawn[0].gdt_labels.push(DrawingLabel {
                        position_mm: above,
                        text: format!("{}X", frames.len()),
                        stack: None,
                        height_mm: lettering.notes,
                    });
                }
                for (segment, d) in drawn.iter().enumerate() {
                    let name = match segment {
                        0 => format!("frame {}", first.id),
                        _ => format!("frame {} segment {}", first.id, segment + 1),
                    };
                    presentations[segment] = Some(plane.presentation(name, d)?);
                }
            }
            let mut tolerances = Vec::with_capacity(segments.len());
            let mut problem = None;
            for (segment, control) in segments.iter().enumerate() {
                let mut datums = Vec::new();
                for (compartment, reference) in control.datums.iter().enumerate() {
                    for member in reference.members() {
                        let Some(&datum) = datum_index.get(member) else {
                            problem = Some(format!("datum feature '{member}' is not exported"));
                            break;
                        };
                        if plan.datums[datum].target.output != &attachment.output {
                            problem =
                                Some(format!("datum feature '{member}' lies on another output"));
                            break;
                        }
                        datums.push(PlannedReference {
                            datum,
                            compartment: compartment as u32,
                            boundary: reference.boundary,
                            translation: reference.translation,
                        });
                    }
                }
                let mut unrepresented = Vec::new();
                for modifier in &control.modifiers {
                    match modifier {
                        ToleranceModifier::ContinuousFeature => {
                            unrepresented.push(PmiProperty::ContinuousFeature)
                        }
                        ToleranceModifier::DynamicProfile => {
                            unrepresented.push(PmiProperty::DynamicProfile)
                        }
                        ToleranceModifier::SeparateRequirement => {
                            unrepresented.push(PmiProperty::SeparateRequirement)
                        }
                        ToleranceModifier::FreeState | ToleranceModifier::Statistical => {}
                    }
                }
                if control.unequal.is_some() {
                    unrepresented.push(PmiProperty::UnequalZone);
                }
                if segment > 0 {
                    unrepresented.push(PmiProperty::CompositeRelation);
                }
                unrepresented.sort();
                tolerances.push(PlannedTolerance {
                    id: &first.id,
                    segment,
                    target: target.clone(),
                    characteristic: control.characteristic,
                    value_mm: control.tolerance.normalized()?,
                    diameter: control.zone == GeometricToleranceZone::Diameter,
                    material: control.material,
                    free_state: control.modifiers.contains(&ToleranceModifier::FreeState),
                    statistical: control.modifiers.contains(&ToleranceModifier::Statistical),
                    tangent_plane: control.tangent_plane,
                    projected_mm: control
                        .projected
                        .as_ref()
                        .map(|p| p.height.normalized())
                        .transpose()?,
                    datums,
                    unrepresented,
                    presentation: presentations[segment].take(),
                });
            }
            if let Some(reason) = problem {
                for frame in &frames {
                    skip(&mut plan.skipped, &frame.id, reason.clone());
                }
                continue;
            }
            let range = plan.tolerances.len()..plan.tolerances.len() + tolerances.len();
            for frame in &frames {
                frame_tolerances.insert(&frame.id, range.clone());
            }
            plan.tolerances.extend(tolerances);
            if let Some(limits) = &first.size_limits {
                let (lower_mm, upper_mm) = limits_mm(limits)?;
                for frame in &frames {
                    size_index.insert(&frame.id, plan.sizes.len());
                }
                plan.sizes.push(PlannedSize {
                    id: &first.id,
                    target,
                    lower_mm,
                    upper_mm,
                    drawing: size_drawing,
                });
            }
        }
        // OCCT 7.9's AP242 writer has no surface texture representation.
        for texture in &self.surface_textures {
            skip(
                &mut plan.skipped,
                &texture.id,
                "surface texture: no AP242 representation",
            );
        }
        for view in &self.annotation_views {
            let mut planned = PlannedView {
                id: &view.id,
                name: &view.name,
                axes: view.axes()?,
                datums: Vec::new(),
                tolerances: Vec::new(),
                annotations: Vec::new(),
                omitted: Vec::new(),
                clipping_planes: view
                    .clipping_planes
                    .iter()
                    .map(AnnotationClippingPlane::kept_side)
                    .collect::<Result<_, _>>()?,
                parts: Vec::new(),
                omitted_parts: Vec::new(),
                sizes: Vec::new(),
            };
            for part in &view.parts {
                if graph.node(&part.instance).is_none() {
                    return Err(ModelError::new(format!(
                        "annotation view '{}' shows a part of unknown instance '{}'",
                        view.id, part.instance
                    )));
                }
                if exported(part) {
                    planned.parts.push(part);
                } else {
                    planned.omitted_parts.push(part);
                }
            }
            for id in &view.datum_features {
                match datum_index.get(id.as_str()) {
                    Some(&datum) => {
                        planned.datums.push(datum);
                        planned.annotations.push(id);
                    }
                    None => planned.omitted.push(id),
                }
            }
            // A pattern shows once, however many members the view lists.
            let mut shown = HashSet::new();
            for id in &view.feature_control_frames {
                match frame_tolerances.get(id.as_str()) {
                    Some(range) => {
                        if shown.insert(range.start) {
                            planned.tolerances.extend(range.clone());
                        }
                        planned.annotations.push(id);
                    }
                    None => planned.omitted.push(id),
                }
            }
            // A pattern's size shows once, however many members list it.
            for id in &view.sizes {
                match size_index.get(id.as_str()) {
                    Some(&size) => {
                        if !planned.sizes.contains(&size) {
                            planned.sizes.push(size);
                        }
                    }
                    None => {
                        if !planned.omitted.contains(&id.as_str()) {
                            planned.omitted.push(id);
                        }
                    }
                }
            }
            plan.views.push(planned);
        }
        plan.dataset = self.dataset.as_ref().map(|dataset| PlannedDataset {
            dataset,
            release: self.releases.last(),
        });
        Ok(plan)
    }
}
