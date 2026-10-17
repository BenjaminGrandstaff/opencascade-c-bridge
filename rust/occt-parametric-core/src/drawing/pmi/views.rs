//! Annotation views (schema 145): saved views of a drawing's GD&T on the
//! model, as ASME Y14.41 organizes a dataset's annotations, and the 3D
//! presentation each annotation has in the plane of the first view that
//! shows it.
use super::*;
use crate::assembly::{cross, dot};

/// A named view of the model that shows a set of annotations, exchanged as
/// an AP242 saved view. Its axes follow drawing views: `direction` points
/// toward the viewer and `x_axis` to image right.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingAnnotationView {
    pub id: String,
    /// The saved view's name: 1–64 printable ASCII characters, unique within
    /// the drawing.
    pub name: String,
    pub direction: VectorQuantity,
    /// Perpendicular to `direction`.
    pub x_axis: VectorQuantity,
    /// Size of the annotations this view presents: 1 draws them at their
    /// sheet size in model millimetres.
    #[serde(default = "unit_scale", skip_serializing_if = "is_unit")]
    pub scale: f64,
    /// Datum features shown, by ID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datum_features: Vec<String>,
    /// Feature control frames shown, by ID; one member shows its pattern.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feature_control_frames: Vec<String>,
    /// Section and clipping planes (schema 150): the view keeps only the
    /// region on the kept side of every plane, at most 16.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clipping_planes: Vec<AnnotationClippingPlane>,
    /// Model visibility (schema 173): the instance outputs the view shows,
    /// each at most once. A view that lists none leaves visibility to the
    /// reader; one that lists some shows those parts alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<InstanceOutputRef>,
}

/// A cutting plane of an annotation view, as for section drawing views:
/// the view keeps the side `normal` points into when `keep_positive`, and
/// the other side otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnnotationClippingPlane {
    pub origin: VectorQuantity,
    pub normal: VectorQuantity,
    pub keep_positive: bool,
}

impl AnnotationClippingPlane {
    /// The plane's origin in millimetres and the unit normal into the side
    /// the view keeps.
    pub fn kept_side(&self) -> Result<(Vec3, Vec3), ModelError> {
        let origin = self.origin.normalized(Dimension::Length)?;
        if ![origin.x, origin.y, origin.z].iter().all(|v| v.is_finite()) {
            return Err(ModelError::new("clipping plane origin must be finite"));
        }
        let normal = axis(self.normal)?;
        let sign = if self.keep_positive { 1.0 } else { -1.0 };
        Ok((
            Vec3::new(origin.x, origin.y, origin.z),
            Vec3::new(sign * normal.x, sign * normal.y, sign * normal.z),
        ))
    }
}

fn unit_scale() -> f64 {
    1.0
}

fn is_unit(value: &f64) -> bool {
    *value == 1.0
}

/// Unit axes of an annotation view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnnotationViewAxes {
    /// Toward the viewer.
    pub direction: Vec3,
    /// Image right.
    pub x_axis: Vec3,
    /// Image up: `direction × x_axis`.
    pub up: Vec3,
}

impl DrawingAnnotationView {
    pub fn axes(&self) -> Result<AnnotationViewAxes, ModelError> {
        let direction = axis(self.direction)?;
        let x_axis = axis(self.x_axis)?;
        let projection = dot(direction, x_axis);
        if projection.abs() > 1e-9 {
            return Err(ModelError::new(format!(
                "annotation view '{}' needs perpendicular direction and x_axis",
                self.id
            )));
        }
        let x_axis = axis(VectorQuantity::scalars(
            x_axis.x - direction.x * projection,
            x_axis.y - direction.y * projection,
            x_axis.z - direction.z * projection,
        ))?;
        Ok(AnnotationViewAxes {
            direction,
            x_axis,
            up: cross(direction, x_axis),
        })
    }
}

/// O(views + listed annotations).
pub(crate) fn validate(definition: &DrawingDefinition) -> Result<(), ModelError> {
    let features: HashSet<&str> = definition
        .datum_features
        .iter()
        .map(|f| f.id.as_str())
        .collect();
    let frames: HashSet<&str> = definition
        .feature_control_frames
        .iter()
        .map(|f| f.id.as_str())
        .collect();
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for view in &definition.annotation_views {
        if view.id.is_empty() || !ids.insert(view.id.as_str()) {
            return Err(ModelError::new("annotation views need unique nonempty IDs"));
        }
        if view.name.is_empty()
            || view.name.len() > 64
            || !view.name.bytes().all(|b| (0x20..0x7f).contains(&b))
            || !names.insert(view.name.as_str())
        {
            return Err(ModelError::new(format!(
                "annotation view '{}' needs a unique name of 1-64 printable ASCII characters",
                view.id
            )));
        }
        view.axes()?;
        if view.clipping_planes.len() > 16 {
            return Err(ModelError::new(format!(
                "annotation view '{}' has more than 16 clipping planes",
                view.id
            )));
        }
        for plane in &view.clipping_planes {
            plane.kept_side()?;
        }
        let mut parts = HashSet::new();
        for part in &view.parts {
            if !parts.insert(part) {
                return Err(ModelError::new(format!(
                    "annotation view '{}' shows part '{}.{}' twice",
                    view.id, part.instance, part.output
                )));
            }
        }
        if !view.scale.is_finite() || view.scale <= 0.0 {
            return Err(ModelError::new(format!(
                "annotation view '{}' needs a positive finite scale",
                view.id
            )));
        }
        for (listed, known, what) in [
            (&view.datum_features, &features, "datum feature"),
            (
                &view.feature_control_frames,
                &frames,
                "feature control frame",
            ),
        ] {
            let mut seen = HashSet::new();
            for id in listed {
                if !known.contains(id.as_str()) {
                    return Err(ModelError::new(format!(
                        "annotation view '{}' shows unknown {what} '{id}'",
                        view.id
                    )));
                }
                if !seen.insert(id.as_str()) {
                    return Err(ModelError::new(format!(
                        "annotation view '{}' shows {what} '{id}' twice",
                        view.id
                    )));
                }
            }
        }
    }
    Ok(())
}

/// An annotation's 3D linework in its annotation plane, in assembly
/// coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct PmiPresentation {
    /// `datum <label>` or `frame <id>`, with ` segment <n>` for a lower
    /// composite segment.
    pub name: String,
    pub polylines: Vec<Vec<Vec3>>,
    /// The anchor the leader points at.
    pub origin: Vec3,
    /// Toward the viewer of the view whose plane it is.
    pub normal: Vec3,
    pub x_direction: Vec3,
}

/// An annotation plane through an anchor, square to a view's direction:
/// sheet millimetres `[x, y]` relative to the anchor map to
/// `anchor + scale · (x · right + y · up)`.
pub(crate) struct AnnotationPlane {
    origin: Vec3,
    axes: AnnotationViewAxes,
    scale: f64,
}

impl AnnotationPlane {
    pub(crate) fn new(view: &DrawingAnnotationView, anchor: Vec3) -> Result<Self, ModelError> {
        Ok(Self {
            origin: anchor,
            axes: view.axes()?,
            scale: view.scale,
        })
    }

    fn point(&self, p: [f64; 2]) -> Vec3 {
        let AnnotationViewAxes { x_axis, up, .. } = self.axes;
        let [x, y] = [p[0] * self.scale, p[1] * self.scale];
        Vec3::new(
            self.origin.x + x * x_axis.x + y * up.x,
            self.origin.y + x * x_axis.y + y * up.y,
            self.origin.z + x * x_axis.z + y * up.z,
        )
    }

    /// The GD&T lines and labels of `d`, the labels as stroke letters, in
    /// this plane. O(lines + characters).
    pub(crate) fn presentation(
        &self,
        name: String,
        d: &GeneratedDrawing,
    ) -> Result<PmiPresentation, ModelError> {
        let mut polylines: Vec<Vec<Vec3>> = d
            .gdt_lines
            .iter()
            .map(|line| line.points_mm.iter().map(|p| self.point(*p)).collect())
            .collect();
        for label in &d.gdt_labels {
            let strokes = strokes::text(&label.text, label.position_mm, label.height_mm)
                .ok_or_else(|| {
                    ModelError::new(format!(
                        "{name} has text '{}' without stroke letters",
                        label.text
                    ))
                })?;
            polylines.extend(
                strokes
                    .into_iter()
                    .map(|stroke| stroke.into_iter().map(|p| self.point(p)).collect()),
            );
        }
        Ok(PmiPresentation {
            name,
            polylines,
            origin: self.origin,
            normal: self.axes.direction,
            x_direction: self.axes.x_axis,
        })
    }
}

/// A saved view to write: the plan's datums and tolerances it shows (every
/// segment of a composite, each pattern once).
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedView<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub axes: AnnotationViewAxes,
    pub datums: Vec<usize>,
    pub tolerances: Vec<usize>,
    /// Shown datum features and frames, by ID, in the view's order.
    pub annotations: Vec<&'a str>,
    /// Listed annotations the plan skipped.
    pub omitted: Vec<&'a str>,
    /// Each plane's origin and its unit normal into the kept side.
    pub clipping_planes: Vec<(Vec3, Vec3)>,
    /// Exported parts the view shows, in its order.
    pub parts: Vec<&'a InstanceOutputRef>,
    /// Listed parts that were not exported.
    pub omitted_parts: Vec<&'a InstanceOutputRef>,
}

/// What of a saved view did not read back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PmiViewLoss {
    /// No camera of the view's name.
    View,
    /// The camera's direction or up.
    Camera,
    /// Its clipping planes.
    Clipping,
    /// The parts it shows.
    Parts,
}

/// One exported saved view and what reading it back recovered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PmiViewReport {
    pub id: String,
    pub name: String,
    /// Exported datum features and frames the view shows, by ID.
    pub annotations: Vec<String>,
    /// Listed annotations that were skipped (see the report's `skipped`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted: Vec<String>,
    /// Listed parts that were not exported, so the view does not show them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted_parts: Vec<InstanceOutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lost: Option<PmiViewLoss>,
    /// Annotations the read view does not show.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lost_annotations: Vec<String>,
}
