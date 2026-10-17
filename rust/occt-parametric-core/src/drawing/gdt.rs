//! Structured drawing GD&T intent and measured inspection of supported controls.
use super::*;
use crate::assembly::{length, unit};
pub mod composite;
pub mod datums;
pub mod inspection;
pub mod position;
pub mod size;
pub mod targets;
pub mod texture;
pub use composite::DrawingCompositeRefinement;
pub use datums::{
    DatumPrecedence, DrawingDatumCoordinateFrame, DrawingDatumReferenceFrame,
    ResolvedDrawingDatumReference, ResolvedDrawingDatumReferenceFrame,
};
pub use inspection::{
    ControlInspection, ControlMeasurement, ControlResult, DecisionRule, InspectionRecord,
    InspectionReport, MAX_INSPECTION_POINTS, MeasuredDatumAxis, MeasuredDatumFrame,
    MeasuredDatumWidth, MeasuredFeature, MeasuredNominal, MeasuredProfile, MeasuredTexture,
    MeasuredThread, MeasurementUncertainty, ProfileMeasurement, RefinementMeasurement,
    TextureInspection, TextureMeasurement, TextureResult, ThreadDiameter, ThreadDiameterCheck,
    ThreadInspection, UncertaintySimulation,
};
pub use position::{
    DrawingPositionMeasurement, DrawingPositionMeasurementResult, PositionSampleEvaluation,
    PositionToleranceAxis,
};
pub use size::{DrawingSizeLimits, FeatureOfSizeKind, GeometricToleranceAllowance};
pub use targets::{DatumTargetShape, DrawingDatumTarget, MAX_DATUM_TARGETS};
pub use texture::{
    DrawingSurfaceTexture, MaterialRemoval, RoughnessLimits, RoughnessParameter, RoughnessUnit,
    STANDARD_CUTOFFS_MM, SurfaceLay, TextureAcceptance, Waviness,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingGdtAttachment {
    pub view: String,
    /// Selected solid output; anchor supplies the current leader attachment.
    pub output: InstanceOutputRef,
    pub anchor: DatumRef,
    pub offset_mm: [f64; 2],
    /// The persistent faces of `output` that are the toleranced feature or
    /// datum feature (schema 144). They make the annotation semantic product
    /// manufacturing information: STEP export with PMI attaches it to these
    /// faces. Each pattern member names its own. A profile or runout control
    /// without a `surface` takes its nominals from them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faces: Option<FaceSelector>,
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
    /// Datum targets on a planar datum feature (schema 166). When present,
    /// they alone establish the datum.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<DrawingDatumTarget>,
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
    /// Controls measured against nominal surface points and normals.
    pub fn nominal_surface(self) -> bool {
        self.profile() || matches!(self, Self::CircularRunout | Self::TotalRunout)
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
    /// Translation modifier (schema 143, ▷): the datum feature simulator
    /// is free to translate from its basic location to engage the feature.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub translation: bool,
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
    /// Faces of the attachment output that a profile or runout control covers
    /// (schema 140), resolved on the regenerated part to supply nominal points
    /// and normals for measured points that lack them. Each pattern member
    /// names its own faces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<FaceSelector>,
    /// Projected tolerance zone (schema 142, Ⓟ) of a position control on a
    /// cylindrical feature: the zone extends `height` beyond the part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected: Option<DrawingProjectedZone>,
    /// Tangent plane (schema 142, Ⓣ) of an orientation control on a plane:
    /// only the plane contacting the surface's high points must lie in the
    /// zone, not every point of the surface.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tangent_plane: bool,
    /// Further modifiers (schema 143), drawn after the tolerance in the
    /// order given.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<ToleranceModifier>,
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
/// Tolerance modifiers without a value (schema 143).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToleranceModifier {
    /// Ⓕ: the tolerance applies with the part in its free state. A
    /// declaration: points are taken as measured.
    FreeState,
    /// ⟨ST⟩: a statistical tolerance, judged on process data; a single
    /// part's measurement is not evaluated against it.
    Statistical,
    /// CF: interrupted surfaces are one continuous feature of size; their
    /// measured points are taken together.
    ContinuousFeature,
    /// △: a dynamic profile zone, free to grow or shrink uniformly, so it
    /// controls form and orientation but not size.
    DynamicProfile,
    /// SEP REQT (schema 167): the control, or its `pattern`, is evaluated
    /// apart from other controls with the same datum references instead of
    /// sharing one datum shift with them (the simultaneous requirement).
    /// Position or profile controls with datums, not composite.
    SeparateRequirement,
}
/// Where a projected tolerance zone lies (schema 142): from a plane of the
/// part, the face the mating part seats on, `height` along that plane's
/// outward normal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingProjectedZone {
    pub height: Quantity,
    /// A plane datum of the control's attachment instance; the primary
    /// datum plane when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
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
/// Room a modifier takes after the tolerance value, mm.
fn modifier_width(m: ToleranceModifier) -> f64 {
    match m {
        ToleranceModifier::Statistical => 9.0,
        ToleranceModifier::ContinuousFeature => 7.0,
        ToleranceModifier::FreeState | ToleranceModifier::DynamicProfile => 6.0,
        // Noted beneath the frame, not in a compartment.
        ToleranceModifier::SeparateRequirement => 0.0,
    }
}
/// Displayed Ⓟ height, in the tolerance's display unit and precision.
fn projected_text(frame: &DrawingFeatureControlFrame) -> Result<Option<String>, ModelError> {
    let Some(projected) = &frame.projected else {
        return Ok(None);
    };
    let mm = projected.height.normalized()?;
    if projected.height.dimension != Dimension::Length || !mm.is_finite() || mm <= 0.0 {
        return Err(ModelError::new(
            "a projected tolerance zone needs a positive finite height",
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
        validate_faces(&feature.attachment, graph)?;
        targets::validate(feature, graph)?;
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
        validate_faces(&frame.attachment, graph)?;
        number(frame)?;
        validate_control(frame)?;
        unequal_text(frame)?;
        projected_text(frame)?;
        validate_modifiers(frame, graph)?;
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
/// Ⓟ applies to position of a cylindrical feature of size, from a plane
/// datum of the attachment instance; Ⓣ to orientation of a plane.
fn validate_modifiers(
    frame: &DrawingFeatureControlFrame,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    if let Some(projected) = &frame.projected {
        if frame.characteristic != GeometricCharacteristic::Position
            || !frame.feature_of_size
            || frame.zone != GeometricToleranceZone::Diameter
        {
            return Err(ModelError::new(
                "a projected tolerance zone applies to position of a cylindrical feature of size",
            ));
        }
        if let Some(from) = &projected.from
            && !matches!(
                graph.datum(&frame.attachment.anchor.instance, from)?,
                ResolvedDatum::Plane { .. }
            )
        {
            return Err(ModelError::new(
                "a projected tolerance zone starts from a plane datum of the attachment instance",
            ));
        }
    }
    let mut seen = HashSet::new();
    for modifier in &frame.modifiers {
        if !seen.insert(*modifier) {
            return Err(ModelError::new("a tolerance modifier is repeated"));
        }
        let applies = match modifier {
            ToleranceModifier::FreeState | ToleranceModifier::Statistical => true,
            ToleranceModifier::ContinuousFeature => frame.feature_of_size,
            ToleranceModifier::DynamicProfile => frame.characteristic.profile(),
            ToleranceModifier::SeparateRequirement => {
                (frame.characteristic == GeometricCharacteristic::Position
                    || frame.characteristic.profile())
                    && !frame.datums.is_empty()
                    && frame.refinement.is_none()
            }
        };
        if !applies {
            return Err(ModelError::new(
                "continuous feature applies to features of size, dynamic profile to profile controls, and a separate requirement to position or profile controls with datums that are not composite",
            ));
        }
    }
    if frame.tangent_plane
        && !matches!(
            frame.characteristic,
            GeometricCharacteristic::Parallelism
                | GeometricCharacteristic::Perpendicularity
                | GeometricCharacteristic::Angularity
        )
    {
        return Err(ModelError::new(
            "a tangent plane applies to parallelism, perpendicularity and angularity",
        ));
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
    if !frame.characteristic.nominal_surface() {
        return Err(ModelError::new(
            "a surface selector applies to profile and runout controls",
        ));
    }
    face_names(surface, &frame.attachment, graph, "profile surface")
}
/// Attachment faces may name only face references of the output instance's
/// family.
fn validate_faces(
    attachment: &DrawingGdtAttachment,
    graph: &InstanceGraph<'_>,
) -> Result<(), ModelError> {
    match &attachment.faces {
        Some(faces) => face_names(faces, attachment, graph, "attachment faces"),
        None => Ok(()),
    }
}
fn face_names(
    selector: &FaceSelector,
    attachment: &DrawingGdtAttachment,
    graph: &InstanceGraph<'_>,
    what: &str,
) -> Result<(), ModelError> {
    let definition = graph.resolve(&attachment.output.instance)?.definition;
    let mut names = Vec::new();
    selector.names(&mut names);
    for (name, _) in names {
        if !definition
            .references
            .iter()
            .any(|r| r.name == name && matches!(r.target, ReferenceTarget::Faces(_)))
        {
            return Err(ModelError::new(format!(
                "{what} names unknown face reference '{name}'"
            )));
        }
    }
    Ok(())
}
fn validate_references(
    datums: &[DrawingDatumReference],
    index: &HashMap<&str, &DrawingDatumFeature>,
) -> Result<(), ModelError> {
    if datums.first().is_some_and(|d| d.translation) {
        return Err(ModelError::new(
            "the primary datum reference has no basic location to translate from",
        ));
    }
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
pub(super) fn line(d: &mut GeneratedDrawing, p: Vec<[f64; 2]>) {
    d.gdt_lines.push(DrawingPolyline {
        points_mm: p,
        hidden: false,
        thin: true,
    });
}
pub(super) fn text(d: &mut GeneratedDrawing, p: [f64; 2], value: String) {
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
pub(super) fn leader(d: &mut GeneratedDrawing, point: [f64; 2], target: [f64; 2], closed: bool) {
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
        draw_datum_feature(f, point, p, d);
        if let Some(view) = views.get(f.attachment.view.as_str()) {
            targets::draw(f, view, graph, d)?;
        }
    }
    for f in effective_frames(frames, reference_frames)? {
        let (point, p) = anchor(&f.attachment, views, graph)?;
        draw_frame(&f, &index, point, p, std::slice::from_mut(d))?;
    }
    Ok(())
}
/// An empty drawing that collects GD&T linework and text alone.
pub(crate) fn scratch(lettering: DrawingLettering) -> GeneratedDrawing {
    GeneratedDrawing {
        curves: Vec::new(),
        gdt_lines: Vec::new(),
        gdt_labels: Vec::new(),
        sheet_lines: Vec::new(),
        sheet_labels: Vec::new(),
        id: String::new(),
        title: String::new(),
        paper_size_mm: [0.0, 0.0],
        polylines: Vec::new(),
        guides: Vec::new(),
        hatches: Vec::new(),
        labels: Vec::new(),
        metadata: BTreeMap::new(),
        generated_variants: 0,
        lettering,
        line_weights: None,
        zones: None,
    }
}
/// A datum feature symbol: its boxed label at `p` and a filled leader to
/// `point`.
pub(crate) fn draw_datum_feature(
    f: &DrawingDatumFeature,
    point: [f64; 2],
    p: [f64; 2],
    d: &mut GeneratedDrawing,
) {
    let width = 4.0 + 3.0 * f.label.len() as f64;
    rectangle(d, p, width);
    text(d, [p[0] + 2.0, p[1] + 2.5], f.label.clone());
    leader(d, point, [p[0] + width / 2.0, p[1]], true);
}
/// A resolved frame at `p` with its leader to `point`, into `d[0]`; or with
/// one drawing per composite segment, each segment's part into its own.
pub(crate) fn draw_frame(
    f: &DrawingFeatureControlFrame,
    index: &HashMap<&str, &DrawingDatumFeature>,
    point: [f64; 2],
    p: [f64; 2],
    d: &mut [GeneratedDrawing],
) -> Result<(), ModelError> {
    if f.refinement.is_some() {
        return composite::append(f, index, point, p, d);
    }
    let d = &mut d[0];
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
                .map_or(0.0, |u| 7.0 + u.len() as f64 * 2.0)
            + projected_text(f)
                .ok()
                .flatten()
                .map_or(0.0, |h| 7.0 + h.len() as f64 * 2.0)
            + if f.tangent_plane { 6.0 } else { 0.0 }
            + f.modifiers.iter().map(|m| modifier_width(*m)).sum::<f64>(),
    ];
    cells.extend(f.datums.iter().map(|r| {
        4.0 + r.label(index).len() as f64 * 3.0
            + if r.boundary != DatumMaterialBoundary::Regardless {
                6.0
            } else {
                0.0
            }
            + if r.translation { 6.0 } else { 0.0 }
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
    // Ⓟ with its height, then Ⓣ, after any material modifier.
    let mut mx = tx + value.len() as f64 * 2.0 + 3.0 + if material { 6.0 } else { 0.0 };
    if let Ok(Some(height)) = projected_text(f) {
        circle(d, [mx, p[1] + 4.0], 2.3, 0.0, std::f64::consts::TAU);
        text(d, [mx - 1.1, p[1] + 3.0], "P".into());
        text(d, [mx + 3.5, p[1] + 2.5], height.clone());
        mx += 7.0 + height.len() as f64 * 2.0;
    }
    if f.tangent_plane {
        circle(d, [mx, p[1] + 4.0], 2.3, 0.0, std::f64::consts::TAU);
        text(d, [mx - 1.1, p[1] + 3.0], "T".into());
        mx += 6.0;
    }
    for m in &f.modifiers {
        let y = p[1] + 4.0;
        match m {
            ToleranceModifier::FreeState => {
                circle(d, [mx, y], 2.3, 0.0, std::f64::consts::TAU);
                text(d, [mx - 1.1, y - 1.0], "F".into());
            }
            ToleranceModifier::Statistical => {
                line(
                    d,
                    vec![
                        [mx - 2.3, y],
                        [mx - 1.0, y + 2.3],
                        [mx + 4.0, y + 2.3],
                        [mx + 5.3, y],
                        [mx + 4.0, y - 2.3],
                        [mx - 1.0, y - 2.3],
                        [mx - 2.3, y],
                    ],
                );
                text(d, [mx - 0.6, y - 1.0], "ST".into());
            }
            ToleranceModifier::ContinuousFeature => {
                text(d, [mx - 1.5, y - 1.5], "CF".into());
            }
            ToleranceModifier::SeparateRequirement => {
                let below = p[1] - d.lettering.notes - 1.5;
                text(d, [p[0], below], "SEP REQT".into());
            }
            ToleranceModifier::DynamicProfile => {
                line(
                    d,
                    vec![
                        [mx - 2.0, y - 1.7],
                        [mx + 2.0, y - 1.7],
                        [mx, y + 2.0],
                        [mx - 2.0, y - 1.7],
                    ],
                );
            }
        }
        mx += modifier_width(*m);
    }
    for (i, width) in cells.iter().enumerate().skip(1) {
        if i > 1 || first_divider {
            line(d, vec![[x, p[1]], [x, p[1] + 8.0]]);
        }
        x += width;
        if let Some(r) = f.datums.get(i - 1) {
            let label = r.label(index);
            text(d, [x + 2.0, p[1] + 2.5], label.clone());
            let mut after = x + label.len() as f64 * 3.0 + 5.0;
            if r.boundary != DatumMaterialBoundary::Regardless {
                modifier(
                    d,
                    [after, p[1] + 4.0],
                    r.boundary == DatumMaterialBoundary::Maximum,
                );
                after += 6.0;
            }
            if r.translation {
                // ▷: an open triangle pointing right.
                let y = p[1] + 4.0;
                line(
                    d,
                    vec![
                        [after - 2.0, y - 2.0],
                        [after + 2.0, y],
                        [after - 2.0, y + 2.0],
                        [after - 2.0, y - 2.0],
                    ],
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
        .and_then(|b| b.checked_add(features.iter().map(targets::vertex_count).sum()))
        .ok_or_else(|| ModelError::new("GD&T vertex count overflow"))?;
    effective_frames(frames, reference_frames)?
        .iter()
        .try_fold(base, |sum, f| {
            let count = if f.refinement.is_some() {
                // Outline 2n + 3, divider 2, n - 1 row separators of 2, and
                // the leader.
                let segments = composite::segments(f);
                8 + 4 * segments.len()
                    + glyph_vertices(f.characteristic)
                    + segments.iter().map(row_vertices).sum::<usize>()
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
        + if f.projected.is_some() { 33 } else { 0 }
        + if f.tangent_plane { 33 } else { 0 }
        + f.modifiers
            .iter()
            .map(|m| match m {
                ToleranceModifier::FreeState => 33,
                ToleranceModifier::Statistical => 7,
                ToleranceModifier::ContinuousFeature => 0,
                ToleranceModifier::DynamicProfile => 4,
                ToleranceModifier::SeparateRequirement => 0,
            })
            .sum::<usize>()
        + 4 * f.datums.iter().filter(|r| r.translation).count()
        + 33 * f
            .datums
            .iter()
            .filter(|r| r.boundary != DatumMaterialBoundary::Regardless)
            .count()
}
pub(crate) fn effective_frames<'a>(
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
