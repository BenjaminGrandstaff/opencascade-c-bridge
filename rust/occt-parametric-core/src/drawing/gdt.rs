//! Structured drawing GD&T intent and measured inspection of supported controls.
use super::*;
use crate::assembly::{length, unit};
pub mod composite;
pub mod datums;
pub mod inspection;
pub mod position;
pub mod size;
pub mod texture;
pub use composite::DrawingCompositeRefinement;
pub use datums::{
    DatumPrecedence, DrawingDatumCoordinateFrame, DrawingDatumReferenceFrame,
    ResolvedDrawingDatumReference, ResolvedDrawingDatumReferenceFrame,
};
pub use inspection::{
    ControlInspection, ControlMeasurement, ControlResult, InspectionRecord, InspectionReport,
    MAX_INSPECTION_POINTS, MeasuredDatumAxis, MeasuredDatumFrame, MeasuredDatumWidth,
    MeasuredFeature, MeasuredNominal, MeasuredTexture, ProfileMeasurement, RefinementMeasurement,
    TextureInspection, TextureMeasurement, TextureResult,
};
pub use position::{
    DrawingPositionMeasurement, DrawingPositionMeasurementResult, PositionSampleEvaluation,
    PositionToleranceAxis,
};
pub use size::{DrawingSizeLimits, FeatureOfSizeKind, GeometricToleranceAllowance};
pub use texture::{
    DrawingSurfaceTexture, MaterialRemoval, RoughnessLimits, RoughnessParameter, RoughnessUnit,
    STANDARD_CUTOFFS_MM, SurfaceLay, Waviness,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingGdtAttachment {
    pub view: String,
    /// Selected solid output; anchor supplies the current leader attachment.
    pub output: InstanceOutputRef,
    pub anchor: DatumRef,
    pub offset_mm: [f64; 2],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingDatumFeature {
    pub id: String,
    pub label: String,
    /// Explicit manufacturing declaration, not inferred from the anchor kind.
    pub feature_of_size: bool,
    pub attachment: DrawingGdtAttachment,
    /// Internal (hole) or external (shaft) feature of size, which selects the
    /// actual mating envelope that simulates a measured axis datum (schema 135).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_kind: Option<FeatureOfSizeKind>,
    /// Size limits of a datum feature of size; their maximum or least
    /// material limit is the boundary of an MMB or LMB reference, which sets
    /// the allowed datum shift (schema 137). The kind must match `size_kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_limits: Option<DrawingSizeLimits>,
}
impl DrawingDatumFeature {
    /// Internal or external, from `size_kind` or the size limits.
    pub fn kind(&self) -> Option<FeatureOfSizeKind> {
        self.size_kind.or(self.size_limits.as_ref().map(|l| l.kind))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeometricCharacteristic {
    Straightness,
    Flatness,
    Circularity,
    Cylindricity,
    ProfileLine,
    ProfileSurface,
    Parallelism,
    Perpendicularity,
    Angularity,
    Position,
    CircularRunout,
    TotalRunout,
}
impl GeometricCharacteristic {
    fn form(self) -> bool {
        matches!(
            self,
            Self::Straightness | Self::Flatness | Self::Circularity | Self::Cylindricity
        )
    }
    fn profile(self) -> bool {
        matches!(self, Self::ProfileLine | Self::ProfileSurface)
    }
    fn diameter(self) -> bool {
        matches!(
            self,
            Self::Straightness
                | Self::Parallelism
                | Self::Perpendicularity
                | Self::Angularity
                | Self::Position
        )
    }
    fn material(self) -> bool {
        matches!(
            self,
            Self::Straightness
                | Self::Flatness
                | Self::Parallelism
                | Self::Perpendicularity
                | Self::Angularity
                | Self::Position
        )
    }
}
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GeometricToleranceZone {
    #[default]
    Characteristic,
    Diameter,
}
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ToleranceMaterialCondition {
    #[default]
    Regardless,
    Maximum,
    Least,
}
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Hash, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DatumMaterialBoundary {
    #[default]
    Regardless,
    Maximum,
    Least,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingDatumReference {
    pub datum_feature: String,
    #[serde(default)]
    pub boundary: DatumMaterialBoundary,
    /// Further datum features that form a common datum with `datum_feature`
    /// in this compartment, shown as `A-B` (schema 136). Members must be
    /// nominally coplanar planes or coaxial axes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub common_with: Vec<String>,
}
impl DrawingDatumReference {
    /// The compartment's datum features: `datum_feature`, then `common_with`.
    pub fn members(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.datum_feature.as_str())
            .chain(self.common_with.iter().map(String::as_str))
    }
    /// Label text such as `A` or `A-B`.
    pub fn label(&self, index: &HashMap<&str, &DrawingDatumFeature>) -> String {
        self.members()
            .map(|id| index[id].label.as_str())
            .collect::<Vec<_>>()
            .join("-")
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingFeatureControlFrame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_limits: Option<DrawingSizeLimits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datum_reference_frame: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refinement: Option<DrawingCompositeRefinement>,
    /// Pattern ID (schema 138): controls sharing it are one n× control over
    /// several features and must match except for ID and attachment. A
    /// composite refinement relates the features of one pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Unequally disposed profile zone (schema 139, Ⓤ): the part of the
    /// tolerance that lies outside the material, from zero (all inside) to
    /// the whole tolerance (all outside). Equal-bilateral when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unequal: Option<Quantity>,
    /// Faces of the attachment output that a profile control covers (schema
    /// 140), resolved on the regenerated part to supply nominal points and
    /// normals for measured points that lack them. Each pattern member names
    /// its own faces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<FaceSelector>,
    pub id: String,
    pub attachment: DrawingGdtAttachment,
    pub characteristic: GeometricCharacteristic,
    pub tolerance: Quantity,
    pub display_unit: LengthUnit,
    pub precision: u8,
    #[serde(default)]
    pub zone: GeometricToleranceZone,
    #[serde(default)]
    pub material: ToleranceMaterialCondition,
    pub feature_of_size: bool,
    #[serde(default)]
    pub datums: Vec<DrawingDatumReference>,
}
fn anchor(
    a: &DrawingGdtAttachment,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<([f64; 2], [f64; 2]), ModelError> {
    let view = views
        .get(a.view.as_str())
        .ok_or_else(|| ModelError::new("GD&T attachment references an unknown view"))?;
    if !view.outputs.contains(&a.output)
        || a.anchor.instance != a.output.instance
        || graph.is_suppressed(&a.output.instance)
    {
        return Err(ModelError::new(
            "GD&T attachment needs a selected unsuppressed output and its own instance anchor",
        ));
    }
    let offset_length = a.offset_mm[0].hypot(a.offset_mm[1]);
    if !finite_pair(a.offset_mm) || !offset_length.is_finite() || offset_length < 8.0 {
        return Err(ModelError::new(
            "GD&T leader needs a finite paper offset at least 8 mm long",
        ));
    }
    let point = view.paper(view.project(datum_origin(graph, &a.anchor)?)?)?;
    let box_origin = [point[0] + a.offset_mm[0], point[1] + a.offset_mm[1]];
    if !finite_pair(box_origin) || !finite_pair([box_origin[0] + 160.0, box_origin[1] + 8.0]) {
        return Err(ModelError::new(
            "GD&T frame exceeds finite paper coordinates",
        ));
    }
    Ok((point, box_origin))
}
fn number(frame: &DrawingFeatureControlFrame) -> Result<String, ModelError> {
    if frame.tolerance.dimension != Dimension::Length {
        return Err(ModelError::new("geometric tolerance must be a length"));
    }
    let mm = frame.tolerance.normalized()?;
    if !mm.is_finite() || mm <= 0.0 || frame.precision > 8 {
        return Err(ModelError::new(
            "geometric tolerance needs a positive finite length and precision at most eight",
        ));
    }
    let value = mm / frame.display_unit.millimeter_factor();
    let text = format!("{value:.precision$}", precision = frame.precision as usize);
    if text.len() > 20 || text.parse::<f64>().unwrap_or(0.0) <= 0.0 {
        return Err(ModelError::new(
            "geometric tolerance exceeds its display cell or rounds to zero",
        ));
    }
    let unit = match frame.display_unit {
        LengthUnit::Millimeter => "mm",
        LengthUnit::Centimeter => "cm",
        LengthUnit::Meter => "m",
        LengthUnit::Inch => "in",
    };
    Ok(format!("{text} {unit}"))
}
/// Displayed Ⓤ value of an unequally disposed profile zone, in the
/// tolerance's display unit and precision.
fn unequal_text(frame: &DrawingFeatureControlFrame) -> Result<Option<String>, ModelError> {
    let Some(unequal) = &frame.unequal else {
        return Ok(None);
    };
    if !frame.characteristic.profile() {
        return Err(ModelError::new(
            "an unequally disposed zone applies to profile controls",
        ));
    }
    let mm = unequal.normalized()?;
    if unequal.dimension != Dimension::Length
        || !mm.is_finite()
        || mm < 0.0
        || mm > frame.tolerance.normalized()?
    {
        return Err(ModelError::new(
            "an unequally disposed zone needs a length from zero to the tolerance",
        ));
    }
    let value = mm / frame.display_unit.millimeter_factor();
    Ok(Some(format!(
        "{value:.precision$}",
        precision = frame.precision as usize
    )))
}
pub fn validate(
    features: &[DrawingDatumFeature],
    frames: &[DrawingFeatureControlFrame],
    reference_frames: &[DrawingDatumReferenceFrame],
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    let mut index = HashMap::new();
    let mut labels = HashSet::new();
    for feature in features {
        if feature.id.is_empty()
            || index.insert(feature.id.as_str(), feature).is_some()
            || !labels.insert(&feature.label)
            || feature.label.is_empty()
            || feature.label.len() > 3
            || !feature
                .label
                .bytes()
                .all(|c| c.is_ascii_uppercase() && !matches!(c, b'I' | b'O' | b'Q'))
        {
            return Err(ModelError::new(
                "datum features need unique IDs and unique one-to-three-letter uppercase labels excluding I/O/Q",
            ));
        }
        if (feature.size_kind.is_some() || feature.size_limits.is_some())
            && !feature.feature_of_size
        {
            return Err(ModelError::new(
                "datum feature size kind requires a declared feature of size",
            ));
        }
        if let Some(limits) = &feature.size_limits {
            limits.bounds()?;
            if feature.size_kind.is_some_and(|kind| kind != limits.kind) {
                return Err(ModelError::new(
                    "datum feature size limits and size kind disagree",
                ));
            }
        }
        anchor(&feature.attachment, views, graph)?;
    }
    for frame in reference_frames {
        validate_references(&frame.datums, &index)?;
        validate_common(&frame.datums, &index, graph)?;
    }
    let mut ids = HashSet::new();
    let mut patterns: HashMap<&str, DrawingFeatureControlFrame> = HashMap::new();
    let resolved = effective_frames(frames, reference_frames)?;
    for frame in &resolved {
        if frame.id.is_empty() || !ids.insert(&frame.id) {
            return Err(ModelError::new(
                "feature control frames need unique nonempty IDs",
            ));
        }
        anchor(&frame.attachment, views, graph)?;
        number(frame)?;
        validate_control(frame)?;
        unequal_text(frame)?;
        validate_surface(frame, graph)?;
        size::validate(frame)?;
        composite::validate(frame, &index)?;
        validate_references(&frame.datums, &index)?;
        if let Some(pattern) = &frame.pattern {
            // Members match once their own ID, attachment and faces are set
            // aside.
            let shape = |f: &DrawingFeatureControlFrame, attachment: &DrawingGdtAttachment| {
                let mut f = f.clone();
                f.id.clear();
                f.attachment = attachment.clone();
                f.surface = None;
                f
            };
            if let Some(first) = patterns.get(pattern.as_str()) {
                if shape(frame, &first.attachment) != shape(first, &first.attachment) {
                    return Err(ModelError::new(
                        "controls in a pattern must match except for ID, attachment and surface",
                    ));
                }
            } else if pattern.is_empty()
                || (frame.characteristic != GeometricCharacteristic::Position
                    && !frame.characteristic.profile())
            {
                return Err(ModelError::new(
                    "patterns need a nonempty ID on position or profile controls",
                ));
            } else {
                patterns.insert(pattern.as_str(), frame.as_ref().clone());
            }
        }
        validate_common(&frame.datums, &index, graph)?;
    }
    Ok(())
}
/// A profile control's surface selector may name only face references of the
/// attachment instance's family.
fn validate_surface(
    frame: &DrawingFeatureControlFrame,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    let Some(surface) = &frame.surface else {
        return Ok(());
    };
    if !frame.characteristic.profile() {
        return Err(ModelError::new(
            "a surface selector applies to profile controls",
        ));
    }
    let definition = graph.resolve(&frame.attachment.output.instance)?.definition;
    let mut names = Vec::new();
    surface.names(&mut names);
    for (name, _) in names {
        if !definition
            .references
            .iter()
            .any(|r| r.name == name && matches!(r.target, ReferenceTarget::Faces(_)))
        {
            return Err(ModelError::new(format!(
                "profile surface names unknown face reference '{name}'"
            )));
        }
    }
    Ok(())
}
fn validate_references(
    datums: &[DrawingDatumReference],
    index: &HashMap<&str, &DrawingDatumFeature>,
) -> Result<(), ModelError> {
    let mut references = HashSet::new();
    for reference in datums {
        if reference.common_with.len() > 2 {
            return Err(ModelError::new(
                "a common datum has at most three datum features",
            ));
        }
        for member in reference.members() {
            let feature = index.get(member).ok_or_else(|| {
                ModelError::new("feature control frame references an unknown datum feature")
            })?;
            if !references.insert(member) {
                return Err(ModelError::new(
                    "feature control frame repeats a datum feature",
                ));
            }
            if reference.boundary != DatumMaterialBoundary::Regardless && !feature.feature_of_size {
                return Err(ModelError::new(
                    "datum material boundary requires a declared feature of size",
                ));
            }
        }
    }
    Ok(())
}
/// Common datum members must be nominally coplanar planes (same outward
/// normal) or coaxial axes. O(members).
fn validate_common(
    datums: &[DrawingDatumReference],
    index: &HashMap<&str, &DrawingDatumFeature>,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    for reference in datums.iter().filter(|r| !r.common_with.is_empty()) {
        let geometry = |id: &str| {
            let anchor = &index[id].attachment.anchor;
            graph.datum(&anchor.instance, &anchor.datum)
        };
        let first = geometry(&reference.datum_feature)?;
        for member in &reference.common_with {
            let coincident = match (first, geometry(member)?) {
                (
                    ResolvedDatum::Plane { origin, normal },
                    ResolvedDatum::Plane {
                        origin: other,
                        normal: other_normal,
                    },
                ) => {
                    let (n, m) = (unit(normal)?, unit(other_normal)?);
                    dot(n, m) > 1.0 - 1e-9 && dot(subtract(other, origin), n).abs() <= 1e-6
                }
                (
                    ResolvedDatum::Axis { origin, direction },
                    ResolvedDatum::Axis {
                        origin: other,
                        direction: other_direction,
                    },
                ) => {
                    let (d, e) = (unit(direction)?, unit(other_direction)?);
                    length(cross(d, e)) <= 1e-9 && length(cross(subtract(other, origin), d)) <= 1e-6
                }
                _ => false,
            };
            if !coincident {
                return Err(ModelError::new(
                    "common datum features must be nominally coplanar planes or coaxial axes",
                ));
            }
        }
    }
    Ok(())
}
fn validate_control(frame: &DrawingFeatureControlFrame) -> Result<(), ModelError> {
    let c = frame.characteristic;
    if frame.datums.len() > 3
        || (c.form() && !frame.datums.is_empty())
        || (!c.form() && !c.profile() && frame.datums.is_empty())
    {
        return Err(ModelError::new(
            "geometric control has unsupported datum count; form controls have none, orientation/location/runout require one to three",
        ));
    }
    if frame.zone == GeometricToleranceZone::Diameter && (!c.diameter() || !frame.feature_of_size) {
        return Err(ModelError::new(
            "diameter tolerance zone requires an axis control on a declared feature of size",
        ));
    }
    if c == GeometricCharacteristic::Position && frame.zone != GeometricToleranceZone::Diameter {
        return Err(ModelError::new(
            "initial position controls require a diameter zone",
        ));
    }
    if frame.material != ToleranceMaterialCondition::Regardless
        && (!c.material() || !frame.feature_of_size)
    {
        return Err(ModelError::new(
            "tolerance material modifier requires a supported feature-of-size control",
        ));
    }
    Ok(())
}
fn line(d: &mut GeneratedDrawing, p: Vec<[f64; 2]>) {
    d.gdt_lines.push(DrawingPolyline {
        points_mm: p,
        hidden: false,
        thin: true,
    });
}
fn text(d: &mut GeneratedDrawing, p: [f64; 2], value: String) {
    d.gdt_labels.push(DrawingLabel {
        position_mm: p,
        text: value,
        stack: None,
        height_mm: d.lettering.notes,
    });
}
fn rectangle(d: &mut GeneratedDrawing, p: [f64; 2], w: f64) {
    let [x, y] = p;
    line(
        d,
        vec![[x, y], [x + w, y], [x + w, y + 8.0], [x, y + 8.0], [x, y]],
    );
}
fn leader(d: &mut GeneratedDrawing, point: [f64; 2], target: [f64; 2], closed: bool) {
    let delta = [target[0] - point[0], target[1] - point[1]];
    let n = delta[0].hypot(delta[1]);
    let u = [delta[0] / n, delta[1] / n];
    let base = [point[0] + 3.0 * u[0], point[1] + 3.0 * u[1]];
    let a = [base[0] - u[1], base[1] + u[0]];
    let b = [base[0] + u[1], base[1] - u[0]];
    line(
        d,
        if closed {
            vec![a, point, b, a]
        } else {
            vec![a, point, b]
        },
    );
    line(d, vec![base, target]);
}
fn circle(d: &mut GeneratedDrawing, p: [f64; 2], r: f64, start: f64, sweep: f64) {
    line(
        d,
        (0..=32)
            .map(|i| {
                let a = start + sweep * i as f64 / 32.0;
                [p[0] + r * a.cos(), p[1] + r * a.sin()]
            })
            .collect(),
    );
}
fn modifier(d: &mut GeneratedDrawing, p: [f64; 2], maximum: bool) {
    circle(d, p, 2.3, 0.0, std::f64::consts::TAU);
    text(
        d,
        [p[0] - 1.1, p[1] - 1.0],
        if maximum { "M" } else { "L" }.into(),
    );
}
fn glyph(d: &mut GeneratedDrawing, p: [f64; 2], c: GeometricCharacteristic) {
    let [x, y] = p;
    let mut draw =
        |points: &[[f64; 2]]| line(d, points.iter().map(|a| [x + a[0], y + a[1]]).collect());
    match c {
        GeometricCharacteristic::Straightness => draw(&[[-2.5, 0.0], [2.5, 0.0]]),
        GeometricCharacteristic::Flatness => draw(&[
            [-2.5, -1.5],
            [1.0, -1.5],
            [2.5, 1.5],
            [-1.0, 1.5],
            [-2.5, -1.5],
        ]),
        GeometricCharacteristic::Parallelism => {
            draw(&[[-2.5, -2.0], [-0.5, 2.0]]);
            draw(&[[0.5, -2.0], [2.5, 2.0]]);
        }
        GeometricCharacteristic::Perpendicularity => {
            draw(&[[-2.5, -2.0], [2.5, -2.0]]);
            draw(&[[0.0, -2.0], [0.0, 2.0]]);
        }
        GeometricCharacteristic::Angularity => draw(&[[2.5, -2.0], [-2.5, -2.0], [1.0, 2.0]]),
        GeometricCharacteristic::CircularRunout | GeometricCharacteristic::TotalRunout => {
            draw(&[[-2.0, -2.0], [1.5, 2.0], [0.0, 1.7], [1.5, 2.0], [1.3, 0.5]]);
            if c == GeometricCharacteristic::TotalRunout {
                draw(&[[-0.5, -2.0], [3.0, 2.0], [1.5, 1.7], [3.0, 2.0], [2.8, 0.5]]);
                draw(&[[-2.0, -2.0], [-0.5, -2.0]]);
            }
        }
        _ => curved_glyph(d, p, c),
    }
}
fn curved_glyph(d: &mut GeneratedDrawing, p: [f64; 2], c: GeometricCharacteristic) {
    let [x, y] = p;
    match c {
        GeometricCharacteristic::ProfileLine | GeometricCharacteristic::ProfileSurface => {
            circle(d, [x, y - 1.5], 2.5, 0.0, std::f64::consts::PI);
            if c == GeometricCharacteristic::ProfileSurface {
                line(d, vec![[x - 2.5, y - 1.5], [x + 2.5, y - 1.5]]);
            }
        }
        _ => {
            circle(d, p, 2.0, 0.0, std::f64::consts::TAU);
            if c == GeometricCharacteristic::Position {
                line(d, vec![[x - 3.0, y], [x + 3.0, y]]);
                line(d, vec![[x, y - 3.0], [x, y + 3.0]]);
            }
            if c == GeometricCharacteristic::Cylindricity {
                for dx in [-2.3, 2.3] {
                    line(d, vec![[x + dx - 0.6, y - 2.5], [x + dx + 0.6, y + 2.5]]);
                }
            }
        }
    }
}
pub fn append(
    features: &[DrawingDatumFeature],
    frames: &[DrawingFeatureControlFrame],
    reference_frames: &[DrawingDatumReferenceFrame],
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
    d: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let index: HashMap<_, _> = features.iter().map(|f| (f.id.as_str(), f)).collect();
    for f in features {
        let (point, p) = anchor(&f.attachment, views, graph)?;
        let width = 4.0 + 3.0 * f.label.len() as f64;
        rectangle(d, p, width);
        text(d, [p[0] + 2.0, p[1] + 2.5], f.label.clone());
        leader(d, point, [p[0] + width / 2.0, p[1]], true);
    }
    for f in effective_frames(frames, reference_frames)? {
        append_frame(&f, &index, views, graph, d)?;
    }
    Ok(())
}
fn append_frame(
    f: &DrawingFeatureControlFrame,
    index: &HashMap<&str, &DrawingDatumFeature>,
    views: &HashMap<&str, &DrawingView>,
    graph: &InstanceGraph<'_>,
    d: &mut GeneratedDrawing,
) -> Result<(), ModelError> {
    let (point, p) = anchor(&f.attachment, views, graph)?;
    if f.refinement.is_some() {
        return composite::append(f, index, point, p, d);
    }
    let value = number(f)?;
    let cells = cell_widths(f, &value, index);
    rectangle(d, p, cells.iter().sum());
    leader(d, point, p, false);
    glyph(d, [p[0] + 4.0, p[1] + 4.0], f.characteristic);
    append_row(f, p, &value, &cells, index, d, true);
    Ok(())
}
fn cell_widths(
    f: &DrawingFeatureControlFrame,
    value: &str,
    index: &HashMap<&str, &DrawingDatumFeature>,
) -> Vec<f64> {
    let mut cells = vec![
        8.0,
        4.0 + value.len() as f64 * 2.0
            + if f.zone == GeometricToleranceZone::Diameter {
                6.0
            } else {
                0.0
            }
            + if f.material != ToleranceMaterialCondition::Regardless {
                6.0
            } else {
                0.0
            }
            + unequal_text(f)
                .ok()
                .flatten()
                .map_or(0.0, |u| 7.0 + u.len() as f64 * 2.0),
    ];
    cells.extend(f.datums.iter().map(|r| {
        4.0 + r.label(index).len() as f64 * 3.0
            + if r.boundary != DatumMaterialBoundary::Regardless {
                6.0
            } else {
                0.0
            }
    }));
    cells
}
fn append_row(
    f: &DrawingFeatureControlFrame,
    p: [f64; 2],
    value: &str,
    cells: &[f64],
    index: &HashMap<&str, &DrawingDatumFeature>,
    d: &mut GeneratedDrawing,
    first_divider: bool,
) {
    let diameter = f.zone == GeometricToleranceZone::Diameter;
    let material = f.material != ToleranceMaterialCondition::Regardless;
    let mut x = p[0] + 8.0;
    let mut tx = x + 2.0;
    if diameter {
        circle(d, [tx + 2.0, p[1] + 4.0], 1.8, 0.0, std::f64::consts::TAU);
        line(d, vec![[tx, p[1] + 1.5], [tx + 4.0, p[1] + 6.5]]);
        tx += 6.0;
    }
    text(d, [tx, p[1] + 2.5], value.to_owned());
    if material {
        modifier(
            d,
            [tx + value.len() as f64 * 2.0 + 3.0, p[1] + 4.0],
            f.material == ToleranceMaterialCondition::Maximum,
        );
    }
    if let Ok(Some(unequal)) = unequal_text(f) {
        let ux = tx + value.len() as f64 * 2.0 + 3.0;
        circle(d, [ux, p[1] + 4.0], 2.3, 0.0, std::f64::consts::TAU);
        text(d, [ux - 1.1, p[1] + 3.0], "U".into());
        text(d, [ux + 3.5, p[1] + 2.5], unequal);
    }
    for (i, width) in cells.iter().enumerate().skip(1) {
        if i > 1 || first_divider {
            line(d, vec![[x, p[1]], [x, p[1] + 8.0]]);
        }
        x += width;
        if let Some(r) = f.datums.get(i - 1) {
            let label = r.label(index);
            text(d, [x + 2.0, p[1] + 2.5], label.clone());
            if r.boundary != DatumMaterialBoundary::Regardless {
                modifier(
                    d,
                    [x + label.len() as f64 * 3.0 + 5.0, p[1] + 4.0],
                    r.boundary == DatumMaterialBoundary::Maximum,
                );
            }
        }
    }
}

/// O(annotations + datum references), reserves the exact generated line vertices.
pub fn vertex_count(
    features: &[DrawingDatumFeature],
    frames: &[DrawingFeatureControlFrame],
    reference_frames: &[DrawingDatumReferenceFrame],
) -> Result<usize, ModelError> {
    let base = features
        .len()
        .checked_mul(11)
        .ok_or_else(|| ModelError::new("GD&T vertex count overflow"))?;
    effective_frames(frames, reference_frames)?
        .iter()
        .try_fold(base, |sum, f| {
            let count = if f.refinement.is_some() {
                16 + glyph_vertices(f.characteristic)
                    + row_vertices(f)
                    + row_vertices(&composite::refined(f))
            } else {
                12 + glyph_vertices(f.characteristic) + row_vertices(f)
            };
            sum.checked_add(count)
                .ok_or_else(|| ModelError::new("GD&T vertex count overflow"))
        })
}
fn glyph_vertices(c: GeometricCharacteristic) -> usize {
    match c {
        GeometricCharacteristic::Straightness => 2,
        GeometricCharacteristic::Flatness => 5,
        GeometricCharacteristic::Circularity | GeometricCharacteristic::ProfileLine => 33,
        GeometricCharacteristic::Cylindricity | GeometricCharacteristic::Position => 37,
        GeometricCharacteristic::ProfileSurface => 35,
        GeometricCharacteristic::Parallelism | GeometricCharacteristic::Perpendicularity => 4,
        GeometricCharacteristic::Angularity => 3,
        GeometricCharacteristic::CircularRunout => 5,
        GeometricCharacteristic::TotalRunout => 12,
    }
}
fn row_vertices(f: &DrawingFeatureControlFrame) -> usize {
    2 * f.datums.len()
        + if f.zone == GeometricToleranceZone::Diameter {
            35
        } else {
            0
        }
        + if f.material != ToleranceMaterialCondition::Regardless {
            33
        } else {
            0
        }
        + if f.unequal.is_some() { 33 } else { 0 }
        + 33 * f
            .datums
            .iter()
            .filter(|r| r.boundary != DatumMaterialBoundary::Regardless)
            .count()
}
fn effective_frames<'a>(
    frames: &'a [DrawingFeatureControlFrame],
    reference_frames: &'a [DrawingDatumReferenceFrame],
) -> Result<Vec<std::borrow::Cow<'a, DrawingFeatureControlFrame>>, ModelError> {
    let mut index = HashMap::new();
    for frame in reference_frames {
        if frame.id.is_empty()
            || frame.datums.is_empty()
            || frame.datums.len() > 3
            || index.insert(frame.id.as_str(), frame).is_some()
        {
            return Err(ModelError::new(
                "datum reference frames need unique IDs and one to three ordered datums",
            ));
        }
    }
    frames
        .iter()
        .map(|f| {
            let Some(id) = &f.datum_reference_frame else {
                return Ok(std::borrow::Cow::Borrowed(f));
            };
            if !f.datums.is_empty() {
                return Err(ModelError::new(
                    "use either inline datums or a named datum reference frame",
                ));
            }
            let frame = index
                .get(id.as_str())
                .ok_or_else(|| ModelError::new("unknown named datum reference frame"))?;
            let mut resolved = f.clone();
            resolved.datums = frame.datums.clone();
            resolved.datum_reference_frame = None;
            Ok(std::borrow::Cow::Owned(resolved))
        })
        .collect()
}
