//! Measured inspection: datum simulators fitted to measured points and
//! conformance of planar form/orientation, round form (circularity and
//! cylindricity, minimum zone) and axis position controls.
//!
//! Measured points are in model coordinates at the current poses, as from a
//! coordinate measuring machine aligned to the part; the residual misalignment
//! must be small because fits start from the nominal orientations. Nominal
//! datum plane normals point out of the material, so each datum simulator
//! contacts the measured high points on that side (constrained L∞).
use super::*;
use crate::assembly::{add, length, scale, unit};
pub mod fit;
use fit::{Fit, circle_zone, cylinder_zone, fit_circle, fit_cylinder, fit_plane, fit_width};

/// Upper bound on measured points in one record, which bounds memory and time.
pub const MAX_INSPECTION_POINTS: usize = 10_000_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredFeature {
    /// Drawing datum feature ID or feature control frame ID.
    pub id: String,
    pub points_mm: Vec<[f64; 3]>,
}

/// Measured points for one part against one drawing's GD&T.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionRecord {
    pub drawing: String,
    #[serde(default)]
    pub datum_features: Vec<MeasuredFeature>,
    #[serde(default)]
    pub controls: Vec<MeasuredFeature>,
    /// Measured roughness values by drawing surface texture ID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_textures: Vec<MeasuredTexture>,
}

/// Roughness readings for one surface texture requirement, each in the
/// requirement's parameter and unit (for example several Ra traces in µm).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredTexture {
    pub id: String,
    pub values: Vec<f64>,
}

/// Measured roughness against a requirement by the maximum rule: every
/// reading must be at or below the maximum and at or above any minimum.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextureMeasurement {
    pub readings: usize,
    pub highest: f64,
    pub lowest: f64,
    pub maximum: f64,
    pub minimum: Option<f64>,
    pub conforms: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum TextureResult {
    Evaluated(TextureMeasurement),
    NotMeasured,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextureInspection {
    pub texture: String,
    pub result: TextureResult,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ControlMeasurement {
    /// Width of the measured zone: plane separation, radial separation of the
    /// minimum-zone circles or cylinders, or twice the radial axis offset.
    pub deviation_mm: f64,
    pub tolerance_mm: f64,
    /// Material-condition bonus from the measured size; zero regardless of size.
    pub bonus_mm: f64,
    /// Actual mating size of a feature of size, when evaluated.
    pub actual_size_mm: Option<f64>,
    pub size_conforms: Option<bool>,
    pub conforms: bool,
    /// Radial datum shift applied before measuring (schema 137): one shift
    /// shared by every position control with identical datum references.
    pub datum_shift_mm: f64,
    /// Largest radial datum shift the referenced material boundaries allow.
    pub datum_shift_limit_mm: f64,
    /// The lower (feature-relating) segment of a composite position control
    /// (schema 138); `conforms` covers both segments.
    pub refinement: Option<RefinementMeasurement>,
}

/// A composite control's feature-relating segment: the pattern may translate,
/// and rotate unless a refinement datum plane fixes it, as one rigid group.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RefinementMeasurement {
    pub deviation_mm: f64,
    pub tolerance_mm: f64,
    pub bonus_mm: f64,
    pub conforms: bool,
    /// Translation and rotation (about the measured frame's z axis, the
    /// primary datum normal) of the pattern's true positions that best relate
    /// its features, shared by every member of the pattern.
    pub pattern_translation_mm: f64,
    pub pattern_rotation_rad: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum ControlResult {
    Evaluated(ControlMeasurement),
    NotMeasured,
    /// The control or its measurements are outside what this evaluator supports.
    NotEvaluated(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ControlInspection {
    pub control: String,
    pub result: ControlResult,
}

/// A complete three-plane frame established from measured datum simulators.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasuredDatumFrame {
    pub datum_features: Vec<String>,
    pub frame: DrawingDatumCoordinateFrame,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InspectionReport {
    pub drawing: String,
    pub datum_frames: Vec<MeasuredDatumFrame>,
    /// Named datum reference frames whose last datum is a measured axis.
    pub datum_axes: Vec<MeasuredDatumAxis>,
    /// Width datum features simulated in named frames, once each.
    pub datum_widths: Vec<MeasuredDatumWidth>,
    /// One entry per effective feature control frame, in drawing order.
    pub controls: Vec<ControlInspection>,
    /// One entry per surface texture requirement, in drawing order.
    pub surface_textures: Vec<TextureInspection>,
}

impl InspectionReport {
    /// True only when every control and surface texture was evaluated and
    /// conforms.
    pub fn conforms(&self) -> bool {
        self.controls
            .iter()
            .all(|c| matches!(&c.result, ControlResult::Evaluated(m) if m.conforms))
            && self
                .surface_textures
                .iter()
                .all(|t| matches!(&t.result, TextureResult::Evaluated(m) if m.conforms))
    }
}

/// A datum simulator: a contacting plane (point, outward normal) or the axis
/// of an actual mating envelope (point, unit direction, mating size).
#[derive(Clone, Debug, PartialEq)]
enum Simulator {
    /// Point, normal, and the mating width of a width datum feature of size
    /// whose simulator is the envelope's center plane.
    Plane(Vec3, Vec3, Option<f64>),
    /// Point, unit direction, mating sizes, and the largest radial datum
    /// shift its material boundary allows (zero regardless of boundary).
    Axis(Vec3, Vec3, Vec<f64>, f64),
}
type Simulators = Vec<Simulator>;
/// A datum precedence prefix: each compartment's members and boundary.
type SimulatorKey = Vec<(Vec<String>, DatumMaterialBoundary)>;
impl Simulator {
    fn plane_normal(&self) -> Result<Vec3, String> {
        match self {
            Self::Plane(_, normal, _) => Ok(*normal),
            Self::Axis(..) => Err("orientation to an axis datum is not evaluated".into()),
        }
    }
}

/// A datum center plane established from a measured width datum feature of
/// size (slot or tab).
#[derive(Clone, Debug, PartialEq)]
pub struct MeasuredDatumWidth {
    pub datum_feature: String,
    /// A point on the center plane and its unit normal.
    pub point_mm: Vec3,
    pub normal: Vec3,
    /// Actual mating width.
    pub mating_size_mm: f64,
}

/// A datum axis established from a measured cylindrical datum feature of size.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasuredDatumAxis {
    pub datum_features: Vec<String>,
    pub origin_mm: Vec3,
    pub direction: Vec3,
    /// Actual mating size (diameter) of each axis datum feature, in order
    /// (two for a common datum axis).
    pub mating_sizes_mm: Vec<f64>,
}

struct Context<'a, 'g> {
    graph: &'a InstanceGraph<'g>,
    features: HashMap<&'a str, &'a DrawingDatumFeature>,
    measured_datums: HashMap<&'a str, Vec<Vec3>>,
    /// Simulator planes keyed by datum precedence prefix; each prefix is fitted once.
    simulators: HashMap<SimulatorKey, Result<Simulators, String>>,
    /// Fitted position offsets by control ID, each fitted once.
    positions: HashMap<String, Result<PositionData, String>>,
    /// Shared datum shift by control ID (simultaneous requirement).
    shifts: HashMap<String, [f64; 2]>,
    /// Composite lower-segment results by control ID, fitted per pattern.
    refinements: HashMap<String, RefinementMeasurement>,
}

impl DrawingDefinition {
    /// Evaluates measured points against this drawing's controls.
    ///
    /// Datum simulators are fitted once per distinct datum precedence prefix
    /// and shared by every control that references it, so time is
    /// O(controls · datums + Σ fit cost), each fit O(linearizations · pivots ·
    /// points); memory is O(points). Record errors (unknown IDs, non-finite or
    /// too many points) fail the call; unsupported controls are reported.
    pub fn evaluate_inspection(
        &self,
        graph: &InstanceGraph<'_>,
        record: &InspectionRecord,
    ) -> Result<InspectionReport, ModelError> {
        self.validate(graph)?;
        if record.drawing != self.id {
            return Err(ModelError::new(
                "inspection record names a different drawing",
            ));
        }
        let frames = effective_frames(&self.feature_control_frames, &self.datum_reference_frames)?;
        let control_ids: HashSet<&str> = frames.iter().map(|f| f.id.as_str()).collect();
        let features: HashMap<_, _> = self
            .datum_features
            .iter()
            .map(|f| (f.id.as_str(), f))
            .collect();
        let mut total = 0usize;
        let measured_datums = measured(&record.datum_features, &mut total, |id| {
            features.contains_key(id)
        })?;
        let measured_controls =
            measured(&record.controls, &mut total, |id| control_ids.contains(id))?;
        let mut context = Context {
            graph,
            features,
            measured_datums,
            simulators: HashMap::new(),
            positions: HashMap::new(),
            shifts: HashMap::new(),
            refinements: HashMap::new(),
        };
        // Simultaneous requirement: position controls with identical datum
        // references (features, precedence and boundaries) share one datum
        // shift. Groups are keyed by their serialized references.
        let mut groups: BTreeMap<String, Vec<&DrawingFeatureControlFrame>> = BTreeMap::new();
        for frame in &frames {
            if frame.characteristic == GeometricCharacteristic::Position
                && measured_controls.contains_key(frame.id.as_str())
                && frame
                    .datums
                    .iter()
                    .any(|d| d.boundary != DatumMaterialBoundary::Regardless)
            {
                let key = serde_json::to_string(&frame.datums)
                    .map_err(|e| ModelError::new(e.to_string()))?;
                groups.entry(key).or_default().push(frame);
            }
        }
        for group in groups.values() {
            let mut members = Vec::with_capacity(group.len());
            let mut limit: f64 = 0.0;
            for frame in group {
                let points = &measured_controls[frame.id.as_str()];
                let (Ok(data), Ok(tolerance)) = (
                    context.position_data(frame, points),
                    frame.tolerance.normalized(),
                ) else {
                    continue;
                };
                limit = limit.max(data.shift_limit);
                members.push((
                    frame.id.clone(),
                    (data.offset, 0.5 * (tolerance + data.bonus)),
                ));
            }
            let zones: Vec<_> = members.iter().map(|(_, zone)| *zone).collect();
            let shift = common_shift(&zones, limit);
            for (id, _) in members {
                context.shifts.insert(id, shift);
            }
        }
        // Composite position: each pattern's feature-relating segment moves
        // its true positions as one rigid group.
        let mut patterns: BTreeMap<&str, Vec<&DrawingFeatureControlFrame>> = BTreeMap::new();
        for frame in &frames {
            if frame.characteristic == GeometricCharacteristic::Position
                && frame.refinement.is_some()
                && measured_controls.contains_key(frame.id.as_str())
            {
                let key = frame.pattern.as_deref().unwrap_or(frame.id.as_str());
                patterns.entry(key).or_default().push(frame);
            }
        }
        for group in patterns.values() {
            context.refine_pattern(group, &measured_controls);
        }
        let mut controls = Vec::with_capacity(frames.len());
        for frame in &frames {
            let result = match measured_controls.get(frame.id.as_str()) {
                None => ControlResult::NotMeasured,
                Some(points) => match context.evaluate(frame, points) {
                    Ok(m) => ControlResult::Evaluated(m),
                    Err(reason) => ControlResult::NotEvaluated(reason),
                },
            };
            controls.push(ControlInspection {
                control: frame.id.clone(),
                result,
            });
        }
        let mut datum_frames = Vec::new();
        let mut datum_axes = Vec::new();
        let mut datum_widths: Vec<MeasuredDatumWidth> = Vec::new();
        for reference in &self.datum_reference_frames {
            if let Ok(simulators) = context.simulators(&reference.datums) {
                for (datum, simulator) in reference.datums.iter().zip(simulators) {
                    if let Simulator::Plane(point, normal, Some(width)) = simulator
                        && !datum_widths
                            .iter()
                            .any(|w| w.datum_feature == datum.datum_feature)
                    {
                        datum_widths.push(MeasuredDatumWidth {
                            datum_feature: datum.datum_feature.clone(),
                            point_mm: point,
                            normal,
                            mating_size_mm: width,
                        });
                    }
                }
            }
            let ids: Vec<String> = reference
                .datums
                .iter()
                .flat_map(|d| d.members().map(str::to_owned))
                .collect();
            if let Ok(frame) = context.measured_frame(&reference.datums) {
                datum_frames.push(MeasuredDatumFrame {
                    datum_features: ids,
                    frame,
                });
            } else if let Ok(Some(Simulator::Axis(origin, direction, sizes, _))) = context
                .simulators(&reference.datums)
                .map(|s| s.last().cloned())
            {
                datum_axes.push(MeasuredDatumAxis {
                    datum_features: ids,
                    origin_mm: origin,
                    direction,
                    mating_sizes_mm: sizes,
                });
            }
        }
        let surface_textures = self.inspect_textures(&record.surface_textures, &mut total)?;
        Ok(InspectionReport {
            drawing: self.id.clone(),
            datum_frames,
            datum_axes,
            datum_widths,
            controls,
            surface_textures,
        })
    }

    /// Compares readings with each texture requirement. Unknown or repeated
    /// IDs, empty or invalid readings, and the shared reading budget fail the
    /// record. O(textures + readings).
    fn inspect_textures(
        &self,
        measured: &[MeasuredTexture],
        total: &mut usize,
    ) -> Result<Vec<TextureInspection>, ModelError> {
        let mut readings: HashMap<&str, &[f64]> = HashMap::with_capacity(measured.len());
        for texture in measured {
            *total = total.saturating_add(texture.values.len());
            if *total > MAX_INSPECTION_POINTS {
                return Err(ModelError::new(
                    "inspection record exceeds the measured-point budget",
                ));
            }
            if !self.surface_textures.iter().any(|t| t.id == texture.id) {
                return Err(ModelError::new(format!(
                    "inspection record references unknown surface texture '{}'",
                    texture.id
                )));
            }
            if texture.values.is_empty()
                || !texture.values.iter().all(|v| v.is_finite() && *v >= 0.0)
            {
                return Err(ModelError::new(
                    "roughness readings must be nonempty, finite and nonnegative",
                ));
            }
            if readings
                .insert(texture.id.as_str(), &texture.values)
                .is_some()
            {
                return Err(ModelError::new(
                    "inspection record repeats a surface texture",
                ));
            }
        }
        Ok(self
            .surface_textures
            .iter()
            .map(|texture| TextureInspection {
                texture: texture.id.clone(),
                result: match readings.get(texture.id.as_str()) {
                    None => TextureResult::NotMeasured,
                    Some(values) => {
                        let highest = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                        let lowest = values.iter().copied().fold(f64::INFINITY, f64::min);
                        let limits = &texture.roughness;
                        TextureResult::Evaluated(TextureMeasurement {
                            readings: values.len(),
                            highest,
                            lowest,
                            maximum: limits.maximum,
                            minimum: limits.minimum,
                            conforms: highest <= limits.maximum
                                && limits.minimum.is_none_or(|minimum| lowest >= minimum),
                        })
                    }
                },
            })
            .collect())
    }
}

/// Indexes finite measured points by feature ID within the shared point budget.
fn measured<'a>(
    list: &'a [MeasuredFeature],
    total: &mut usize,
    known: impl Fn(&str) -> bool,
) -> Result<HashMap<&'a str, Vec<Vec3>>, ModelError> {
    let mut map = HashMap::with_capacity(list.len());
    for m in list {
        *total = total.saturating_add(m.points_mm.len());
        if *total > MAX_INSPECTION_POINTS {
            return Err(ModelError::new(
                "inspection record exceeds the measured-point budget",
            ));
        }
        if !known(&m.id) {
            return Err(ModelError::new(format!(
                "inspection record references unknown feature '{}'",
                m.id
            )));
        }
        let points = m
            .points_mm
            .iter()
            .map(|p| {
                if p.iter().all(|v| v.is_finite()) {
                    Ok(Vec3::new(p[0], p[1], p[2]))
                } else {
                    Err(ModelError::new("measured points must be finite"))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if map.insert(m.id.as_str(), points).is_some() {
            return Err(ModelError::new(
                "inspection record repeats a measured feature",
            ));
        }
    }
    Ok(map)
}

fn plane_datum(geometry: ResolvedDatum, what: &str) -> Result<(Vec3, Vec3), String> {
    match geometry {
        ResolvedDatum::Plane { origin, normal } => {
            Ok((origin, unit(normal).map_err(|e| e.to_string())?))
        }
        _ => Err(format!("{what} needs plane datum geometry")),
    }
}

impl Context<'_, '_> {
    fn nominal(&self, anchor: &DatumRef) -> Result<ResolvedDatum, String> {
        self.graph
            .datum(&anchor.instance, &anchor.datum)
            .map_err(|e| e.to_string())
    }

    /// Nominal plane of a datum feature (normal out of the material).
    fn nominal_datum(&self, id: &str) -> Result<(Vec3, Vec3), String> {
        plane_datum(
            self.nominal(&self.features[id].attachment.anchor)?,
            "measured datum evaluation",
        )
    }

    /// Simulator planes (point, outward normal) for an ordered datum prefix.
    fn simulators(&mut self, datums: &[DrawingDatumReference]) -> Result<Simulators, String> {
        // Common datums are distinct simulators: key by every member.
        // Boundaries change the simulator (and its shift), so they key it too.
        let key: SimulatorKey = datums
            .iter()
            .map(|d| (d.members().map(str::to_owned).collect(), d.boundary))
            .collect();
        if let Some(cached) = self.simulators.get(&key) {
            return cached.clone();
        }
        let result = self.fit_simulators(datums);
        self.simulators.insert(key, result.clone());
        result
    }

    fn fit_simulators(&mut self, datums: &[DrawingDatumReference]) -> Result<Simulators, String> {
        let Some((last, prefix)) = datums.split_last() else {
            return Ok(Vec::new());
        };
        let mut simulators = self.simulators(prefix)?;
        let id = last.datum_feature.as_str();
        let members = last
            .members()
            .map(|member| {
                self.measured_datums
                    .get(member)
                    .map(|points| (self.features[member], points.as_slice()))
                    .ok_or_else(|| format!("datum feature '{member}' was not measured"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let feature = self.features[id];
        if let ResolvedDatum::Axis { direction, .. } = self.nominal(&feature.attachment.anchor)? {
            let simulator = if let [(feature, points)] = members.as_slice() {
                self.axis_simulator(feature, points, direction, &simulators, last.boundary)?
            } else if last.boundary != DatumMaterialBoundary::Regardless {
                return Err("datum shift of a common datum axis is not evaluated".into());
            } else {
                self.common_axis(&members, direction, &simulators)?
            };
            simulators.push(simulator);
            return Ok(simulators);
        }
        // A common plane's simulator contacts every member at once: one
        // constrained-L∞ plane over their union.
        let union: Vec<Vec3> = members
            .iter()
            .flat_map(|(_, p)| p.iter().copied())
            .collect();
        let points = union.as_slice();
        if last.boundary != DatumMaterialBoundary::Regardless {
            return Err("datum shift of planar datum features is not evaluated".into());
        }
        // A tertiary plane after a primary plane and a secondary axis only
        // clocks rotation about that axis: its normal turns about A's normal.
        if let [Simulator::Plane(_, primary, _), Simulator::Axis(..)] = simulators.as_slice() {
            let (_, nominal) = self.nominal_datum(id)?;
            let normal = unit(subtract(nominal, scale(*primary, dot(nominal, *primary))))
                .map_err(|_| "tertiary datum plane is parallel to the primary".to_string())?;
            let primary = *primary;
            simulators.push(self.plane_simulator(&members, &union, normal, &[primary])?);
            return Ok(simulators);
        }
        if simulators.iter().any(|s| matches!(s, Simulator::Axis(..))) {
            return Err("plane datums after an axis datum are not evaluated".into());
        }
        let planes: Vec<Vec3> = simulators
            .iter()
            .map(|s| match s {
                Simulator::Plane(_, n, _) => *n,
                Simulator::Axis(_, d, ..) => *d,
            })
            .collect();
        let (_, nominal) = self.nominal_datum(id)?;
        let (normal, axes) = match planes.as_slice() {
            [] => {
                let (u, v) = perpendiculars(nominal);
                (nominal, vec![u, v])
            }
            [primary] => (
                unit(subtract(nominal, scale(*primary, dot(nominal, *primary))))
                    .map_err(|_| "secondary datum plane is parallel to the primary".to_string())?,
                vec![*primary],
            ),
            [primary, secondary, ..] => {
                let n = cross(*primary, *secondary);
                (
                    if dot(n, nominal) < 0.0 {
                        scale(n, -1.0)
                    } else {
                        n
                    },
                    Vec::new(),
                )
            }
        };
        simulators.push(self.plane_simulator(&members, points, normal, &axes)?);
        Ok(simulators)
    }

    /// A contacting plane (constrained L∞) over `points`, or for a single
    /// width datum feature of size, the center plane of its actual mating
    /// envelope: the widest pair of parallel planes between a slot's faces or
    /// the narrowest pair around a tab's. Points split into the two faces by
    /// side of the nominal center plane. The normal starts at `normal` and
    /// turns only about `axes`, as for any plane datum in that precedence.
    fn plane_simulator(
        &self,
        members: &[(&DrawingDatumFeature, &[Vec3])],
        points: &[Vec3],
        normal: Vec3,
        axes: &[Vec3],
    ) -> Result<Simulator, String> {
        let width =
            |feature: &DrawingDatumFeature| feature.feature_of_size && feature.kind().is_some();
        if members.iter().any(|(feature, _)| width(feature)) {
            let [(feature, points)] = members else {
                return Err("common width datums are not evaluated".into());
            };
            let (origin, nominal) = self.nominal_datum(&feature.id)?;
            let (upper, lower): (Vec<Vec3>, Vec<Vec3>) = points
                .iter()
                .partition(|&&p| dot(subtract(p, origin), nominal) >= 0.0);
            let fit = match feature.kind() {
                Some(FeatureOfSizeKind::Internal) => Fit::Inscribed,
                _ => Fit::Circumscribed,
            };
            let (upper, lower) = if dot(normal, nominal) >= 0.0 {
                (upper, lower)
            } else {
                (lower, upper)
            };
            let (n, offset, size) =
                fit_width(&upper, &lower, normal, axes, fit).map_err(|e| e.to_string())?;
            return Ok(Simulator::Plane(scale(n, offset), n, Some(size)));
        }
        let fitted = fit_plane(points, normal, axes).map_err(|e| e.to_string())?;
        Ok(Simulator::Plane(
            scale(fitted.normal, fitted.contact),
            fitted.normal,
            None,
        ))
    }

    /// Common datum axis of two coaxial datum features of size: the line
    /// through the midpoints of each member's own mating-envelope axis over
    /// its measured length (a CMM construction, not a simultaneous-contact
    /// simulator). Evaluated as the primary datum only.
    fn common_axis(
        &self,
        members: &[(&DrawingDatumFeature, &[Vec3])],
        nominal: Vec3,
        prefix: &[Simulator],
    ) -> Result<Simulator, String> {
        if !prefix.is_empty() || members.len() != 2 {
            return Err(
                "common datum axes are evaluated from two features as the primary datum".into(),
            );
        }
        let nominal = unit(nominal).map_err(|e| e.to_string())?;
        let mut midpoints = Vec::with_capacity(2);
        let mut sizes = Vec::with_capacity(2);
        for (feature, points) in members {
            let Simulator::Axis(origin, direction, size, _) = self.axis_simulator(
                feature,
                points,
                nominal,
                &[],
                DatumMaterialBoundary::Regardless,
            )?
            else {
                unreachable!("axis simulator");
            };
            let (lo, hi) =
                points
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &p| {
                        let t = dot(subtract(p, origin), direction);
                        (lo.min(t), hi.max(t))
                    });
            midpoints.push(add(origin, scale(direction, 0.5 * (lo + hi))));
            sizes.extend(size);
        }
        let mut direction = unit(subtract(midpoints[1], midpoints[0]))
            .map_err(|_| "common datum features share a measured midpoint".to_string())?;
        if dot(direction, nominal) < 0.0 {
            direction = scale(direction, -1.0);
        }
        Ok(Simulator::Axis(midpoints[0], direction, sizes, 0.0))
    }

    /// Axis of the actual mating envelope of a cylindrical datum feature of
    /// size: the largest inscribed cylinder of a hole or the smallest
    /// circumscribed cylinder of a shaft. As primary the axis is free; after
    /// a primary plane it is held perpendicular to that plane.
    fn axis_simulator(
        &self,
        feature: &DrawingDatumFeature,
        points: &[Vec3],
        nominal: Vec3,
        prefix: &[Simulator],
        boundary: DatumMaterialBoundary,
    ) -> Result<Simulator, String> {
        let kind = feature.kind().ok_or_else(|| {
            format!(
                "axis datum feature '{}' needs a declared size kind (internal or external)",
                feature.id
            )
        })?;
        let fit = match kind {
            FeatureOfSizeKind::Internal => Fit::Inscribed,
            FeatureOfSizeKind::External => Fit::Circumscribed,
        };
        let nominal = unit(nominal).map_err(|e| e.to_string())?;
        match prefix {
            [] => {
                if boundary != DatumMaterialBoundary::Regardless {
                    return Err("datum shift of a primary axis datum is not evaluated".into());
                }
                let (origin, direction, radius) =
                    fit_cylinder(points, nominal, fit).map_err(|e| e.to_string())?;
                let direction = if dot(direction, nominal) < 0.0 {
                    scale(direction, -1.0)
                } else {
                    direction
                };
                Ok(Simulator::Axis(origin, direction, vec![2.0 * radius], 0.0))
            }
            [Simulator::Plane(point, normal, _)] => {
                if length(cross(nominal, *normal)) > 1e-9 {
                    return Err(
                        "a secondary axis datum must be nominally perpendicular to the primary plane"
                            .into(),
                    );
                }
                let (u, v) = perpendiculars(*normal);
                let projected: Vec<[f64; 2]> = points
                    .iter()
                    .map(|&p| {
                        let d = subtract(p, *point);
                        [dot(d, u), dot(d, v)]
                    })
                    .collect();
                let (center, radius) = fit_circle(&projected, fit).map_err(|e| e.to_string())?;
                let (center, limit) =
                    self.datum_shift(feature, kind, boundary, &projected, center, radius)?;
                let origin = add(*point, add(scale(u, center[0]), scale(v, center[1])));
                Ok(Simulator::Axis(origin, *normal, vec![2.0 * radius], limit))
            }
            _ => Err(
                "axis datums are evaluated as the primary datum or after one primary plane".into(),
            ),
        }
    }

    /// Reference center and largest radial datum shift of a secondary axis
    /// datum feature referenced at a material boundary. MMB: the feature's
    /// mating envelope may float inside (hole) or outside (shaft) a boundary
    /// of its maximum material size. LMB: the minimum-material envelope
    /// floats against a boundary of its least material size and locates the
    /// axis. Sizes come from the feature's `size_limits`; geometric tolerances
    /// that would enlarge the boundary are not included.
    fn datum_shift(
        &self,
        feature: &DrawingDatumFeature,
        kind: FeatureOfSizeKind,
        boundary: DatumMaterialBoundary,
        projected: &[[f64; 2]],
        center: [f64; 2],
        radius: f64,
    ) -> Result<([f64; 2], f64), String> {
        if boundary == DatumMaterialBoundary::Regardless {
            return Ok((center, 0.0));
        }
        let limits = feature.size_limits.as_ref().ok_or_else(|| {
            format!(
                "datum feature '{}' needs size limits for its material boundary",
                feature.id
            )
        })?;
        let (lower, upper) = limits.bounds().map_err(|e| e.to_string())?;
        let internal = kind == FeatureOfSizeKind::Internal;
        let (center, departure) = if boundary == DatumMaterialBoundary::Maximum {
            let size = 2.0 * radius;
            (center, if internal { size - lower } else { upper - size })
        } else {
            let opposite = if internal {
                Fit::Circumscribed
            } else {
                Fit::Inscribed
            };
            let (minimum_center, minimum_radius) =
                fit_circle(projected, opposite).map_err(|e| e.to_string())?;
            let size = 2.0 * minimum_radius;
            (
                minimum_center,
                if internal { upper - size } else { size - lower },
            )
        };
        if departure < -1e-9 {
            return Err(format!(
                "datum feature '{}' violates its {} material boundary",
                feature.id,
                if boundary == DatumMaterialBoundary::Maximum {
                    "maximum"
                } else {
                    "least"
                }
            ));
        }
        Ok((center, 0.5 * departure.max(0.0)))
    }

    fn measured_frame(
        &mut self,
        datums: &[DrawingDatumReference],
    ) -> Result<DrawingDatumCoordinateFrame, String> {
        let planes = self.simulators(datums)?;
        let resolved = self.resolved_frame(datums, Some(&planes))?;
        resolved.nominal_planar_321().map_err(|e| e.to_string())
    }

    fn resolved_frame(
        &self,
        datums: &[DrawingDatumReference],
        planes: Option<&Simulators>,
    ) -> Result<ResolvedDrawingDatumReferenceFrame, String> {
        let datums = datums
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let geometry = match planes {
                    Some(p) => match &p[i] {
                        Simulator::Plane(origin, normal, _) => ResolvedDatum::Plane {
                            origin: *origin,
                            normal: *normal,
                        },
                        Simulator::Axis(origin, direction, ..) => ResolvedDatum::Axis {
                            origin: *origin,
                            direction: *direction,
                        },
                    },
                    None => {
                        self.nominal(&self.features[r.datum_feature.as_str()].attachment.anchor)?
                    }
                };
                Ok(ResolvedDrawingDatumReference {
                    datum_feature: r.datum_feature.clone(),
                    label: self.features[r.datum_feature.as_str()].label.clone(),
                    precedence: [
                        DatumPrecedence::Primary,
                        DatumPrecedence::Secondary,
                        DatumPrecedence::Tertiary,
                    ][i],
                    boundary: r.boundary,
                    geometry,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(ResolvedDrawingDatumReferenceFrame {
            id: String::new(),
            datums,
        })
    }

    fn evaluate(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
    ) -> Result<ControlMeasurement, String> {
        if frame.refinement.is_some() && frame.characteristic != GeometricCharacteristic::Position {
            return Err("composite profile controls are not evaluated".into());
        }
        let tolerance = frame.tolerance.normalized().map_err(|e| e.to_string())?;
        let c = frame.characteristic;
        if c == GeometricCharacteristic::Position {
            return self.position(frame, points, tolerance);
        }
        if matches!(
            c,
            GeometricCharacteristic::Circularity | GeometricCharacteristic::Cylindricity
        ) {
            return self.round_form(frame, points, tolerance);
        }
        if matches!(
            c,
            GeometricCharacteristic::CircularRunout | GeometricCharacteristic::TotalRunout
        ) {
            return self.runout(frame, points, tolerance);
        }
        if frame.material != ToleranceMaterialCondition::Regardless
            || frame.zone != GeometricToleranceZone::Characteristic
        {
            return Err("only planar zones regardless of feature size are evaluated for form and orientation".into());
        }
        let (_, nominal) = plane_datum(self.nominal(&frame.attachment.anchor)?, "planar control")?;
        let fitted = match c {
            GeometricCharacteristic::Flatness => {
                let (u, v) = perpendiculars(nominal);
                fit_plane(points, nominal, &[u, v])
            }
            GeometricCharacteristic::Parallelism
            | GeometricCharacteristic::Perpendicularity
            | GeometricCharacteristic::Angularity => {
                let (normal, axes) = self.oriented(frame, nominal)?;
                fit_plane(points, normal, &axes)
            }
            _ => return Err(format!("{c:?} is not evaluated")),
        }
        .map_err(|e| e.to_string())?;
        Ok(ControlMeasurement {
            deviation_mm: fitted.width,
            tolerance_mm: tolerance,
            bonus_mm: 0.0,
            actual_size_mm: None,
            size_conforms: None,
            conforms: fitted.width <= tolerance,
            datum_shift_mm: 0.0,
            datum_shift_limit_mm: 0.0,
            refinement: None,
        })
    }

    /// Circularity (worst section) or cylindricity about the nominal axis of
    /// the control's axis anchor. Sections are the points sharing a position
    /// along that axis (within 1e-3 mm or 1e-6 of the axial span).
    fn round_form(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
        tolerance: f64,
    ) -> Result<ControlMeasurement, String> {
        if frame.material != ToleranceMaterialCondition::Regardless
            || frame.zone != GeometricToleranceZone::Characteristic
        {
            return Err(
                "circularity and cylindricity are evaluated regardless of feature size".into(),
            );
        }
        let ResolvedDatum::Axis { origin, direction } = self.nominal(&frame.attachment.anchor)?
        else {
            return Err("circularity and cylindricity need axis datum geometry".into());
        };
        let axis = unit(direction).map_err(|e| e.to_string())?;
        let deviation = if frame.characteristic == GeometricCharacteristic::Cylindricity {
            cylinder_zone(points, axis).map_err(|e| e.to_string())?
        } else {
            let (u, v) = perpendiculars(axis);
            let mut placed: Vec<(f64, [f64; 2])> = points
                .iter()
                .map(|&p| {
                    let d = subtract(p, origin);
                    (dot(d, axis), [dot(d, u), dot(d, v)])
                })
                .collect();
            placed.sort_by(|a, b| a.0.total_cmp(&b.0));
            let span = placed.last().map_or(0.0, |p| p.0) - placed.first().map_or(0.0, |p| p.0);
            let gap = (1e-6 * span).max(1e-3);
            let mut worst: f64 = 0.0;
            let mut start = 0;
            for end in 1..=placed.len() {
                if end == placed.len() || placed[end].0 - placed[end - 1].0 > gap {
                    let section: Vec<[f64; 2]> = placed[start..end].iter().map(|p| p.1).collect();
                    worst = worst.max(circle_zone(&section).map_err(|e| e.to_string())?);
                    start = end;
                }
            }
            worst
        };
        Ok(ControlMeasurement {
            deviation_mm: deviation,
            tolerance_mm: tolerance,
            bonus_mm: 0.0,
            actual_size_mm: None,
            size_conforms: None,
            conforms: deviation <= tolerance,
            datum_shift_mm: 0.0,
            datum_shift_limit_mm: 0.0,
            refinement: None,
        })
    }

    /// Runout as full indicator movement about the datum axis established by
    /// the control's datums (an axis datum feature of size, alone or after a
    /// primary plane). Surfaces coaxial with the nominal axis are indicated
    /// radially; faces perpendicular to it, axially. Circular runout is the
    /// worst circular element (points sharing an axial position, or a radius
    /// for faces, within 1e-3 mm or 1e-6 of the span); total runout spans all
    /// points. O(n log n) after the datum fit.
    fn runout(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
        tolerance: f64,
    ) -> Result<ControlMeasurement, String> {
        if frame
            .datums
            .iter()
            .any(|d| d.boundary != DatumMaterialBoundary::Regardless)
        {
            return Err("runout datums are evaluated regardless of material boundary".into());
        }
        let Some(Simulator::Axis(origin, axis, ..)) =
            self.simulators(&frame.datums)?.last().cloned()
        else {
            return Err(
                "runout needs a datum axis from a measured cylindrical datum feature".into(),
            );
        };
        // The surface's relation to the datum axis is design intent: compare
        // nominal directions, not the measured (tilted) axis.
        let datum = frame
            .datums
            .last()
            .map(|d| d.datum_feature.as_str())
            .unwrap_or_default();
        let ResolvedDatum::Axis {
            direction: datum_axis,
            ..
        } = self.nominal(&self.features[datum].attachment.anchor)?
        else {
            return Err(
                "runout needs a datum axis from a measured cylindrical datum feature".into(),
            );
        };
        let datum_axis = unit(datum_axis).map_err(|e| e.to_string())?;
        let radial = match self.nominal(&frame.attachment.anchor)? {
            ResolvedDatum::Axis { direction, .. } => {
                let direction = unit(direction).map_err(|e| e.to_string())?;
                if length(cross(direction, datum_axis)) > 1e-6 {
                    return Err("runout surface axis is not coaxial with the datum axis".into());
                }
                true
            }
            ResolvedDatum::Plane { normal, .. } => {
                let normal = unit(normal).map_err(|e| e.to_string())?;
                if length(cross(normal, datum_axis)) > 1e-6 {
                    return Err("runout face is not perpendicular to the datum axis".into());
                }
                false
            }
            _ => {
                return Err(
                    "runout needs an axis or plane anchor on the controlled surface".into(),
                );
            }
        };
        // (element key, indicated value): radial readings are grouped by axial
        // position, axial (face) readings by radius.
        let mut readings: Vec<(f64, f64)> = points
            .iter()
            .map(|&p| {
                let d = subtract(p, origin);
                let along = dot(d, axis);
                let rho = length(subtract(d, scale(axis, along)));
                if radial { (along, rho) } else { (rho, along) }
            })
            .collect();
        let deviation = if frame.characteristic == GeometricCharacteristic::TotalRunout {
            spread(readings.iter().map(|r| r.1))
        } else {
            let mut worst: f64 = 0.0;
            for element in elements(&mut readings) {
                if element.len() < 3 {
                    return Err("each circular runout element needs at least three points".into());
                }
                worst = worst.max(spread(element.iter().map(|r| r.1)));
            }
            worst
        };
        Ok(ControlMeasurement {
            deviation_mm: deviation,
            tolerance_mm: tolerance,
            bonus_mm: 0.0,
            actual_size_mm: None,
            size_conforms: None,
            conforms: deviation <= tolerance,
            datum_shift_mm: 0.0,
            datum_shift_limit_mm: 0.0,
            refinement: None,
        })
    }

    /// Feature normal at its basic orientation to the measured datums, and the
    /// rotations those datums leave free.
    fn oriented(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        nominal: Vec3,
    ) -> Result<(Vec3, Vec<Vec3>), String> {
        let datums = &frame.datums[..frame.datums.len().min(2)];
        let planes = self.simulators(datums)?;
        let (_, a) = self.nominal_datum(&datums[0].datum_feature)?;
        let measured_a = planes[0].plane_normal()?;
        let reference = match datums.get(1) {
            Some(d) => self.nominal_datum(&d.datum_feature)?.1,
            None => nominal,
        };
        let e = unit(subtract(reference, scale(a, dot(reference, a))))
            .unwrap_or_else(|_| perpendiculars(a).0);
        let measured_e = match planes.get(1) {
            Some(simulator) => simulator.plane_normal()?,
            None => unit(subtract(e, scale(measured_a, dot(e, measured_a))))
                .map_err(|e| e.to_string())?,
        };
        let normal = add(
            add(
                scale(measured_a, dot(nominal, a)),
                scale(measured_e, dot(nominal, e)),
            ),
            scale(cross(measured_a, measured_e), dot(nominal, cross(a, e))),
        );
        let free = planes.len() == 1 && length(cross(normal, measured_a)) > 1e-9;
        Ok((normal, if free { vec![measured_a] } else { Vec::new() }))
    }

    /// Fits one translation, and a rotation unless the refinement's datums
    /// include a plane after the primary, relating every measured member of
    /// a composite pattern, then records each member's refined deviation.
    fn refine_pattern(
        &mut self,
        group: &[&DrawingFeatureControlFrame],
        measured: &HashMap<&str, Vec<Vec3>>,
    ) {
        let Some(refinement) = group.first().and_then(|f| f.refinement.as_ref()) else {
            return;
        };
        let Ok(tolerance) = refinement.tolerance.normalized() else {
            return;
        };
        let rotation_fixed = refinement.datums.iter().skip(1).any(|d| {
            self.features
                .get(d.datum_feature.as_str())
                .and_then(|f| self.nominal(&f.attachment.anchor).ok())
                .is_some_and(|g| matches!(g, ResolvedDatum::Plane { .. }))
        });
        let mut members = Vec::with_capacity(group.len());
        for frame in group {
            if let Ok(data) = self.position_data(frame, &measured[frame.id.as_str()]) {
                members.push((frame.id.clone(), data));
            }
        }
        if members.is_empty() {
            return;
        }
        let count = members.len() as f64;
        let centroid = members.iter().fold([0.0, 0.0], |c, (_, d)| {
            [
                c[0] + d.true_position[0] / count,
                c[1] + d.true_position[1] / count,
            ]
        });
        let fit: Vec<PatternMember> = members
            .iter()
            .map(|(_, d)| PatternMember {
                offset: d.offset,
                lever: [
                    d.true_position[0] - centroid[0],
                    d.true_position[1] - centroid[1],
                ],
                radius: 0.5 * (tolerance + d.bonus),
            })
            .collect();
        let (translation, rotation) = pattern_fit(&fit, !rotation_fixed);
        for ((id, data), member) in members.iter().zip(&fit) {
            let r = member.residual(translation, rotation);
            let deviation = 2.0 * r[0].hypot(r[1]);
            self.refinements.insert(
                id.clone(),
                RefinementMeasurement {
                    deviation_mm: deviation,
                    tolerance_mm: tolerance,
                    bonus_mm: data.bonus,
                    conforms: deviation <= tolerance + data.bonus,
                    pattern_translation_mm: translation[0].hypot(translation[1]),
                    pattern_rotation_rad: rotation,
                },
            );
        }
    }

    fn position(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
        tolerance: f64,
    ) -> Result<ControlMeasurement, String> {
        let data = self.position_data(frame, points)?;
        let shift = self
            .shifts
            .get(frame.id.as_str())
            .copied()
            .unwrap_or([0.0, 0.0]);
        let deviation = 2.0 * (data.offset[0] - shift[0]).hypot(data.offset[1] - shift[1]);
        Ok(ControlMeasurement {
            deviation_mm: deviation,
            tolerance_mm: tolerance,
            bonus_mm: data.bonus,
            actual_size_mm: Some(data.size),
            size_conforms: Some(data.size_conforms),
            conforms: data.size_conforms && deviation <= tolerance + data.bonus,
            datum_shift_mm: shift[0].hypot(shift[1]),
            datum_shift_limit_mm: data.shift_limit,
            refinement: None,
        })
        .map(|mut m| {
            if let Some(refined) = self.refinements.get(frame.id.as_str()) {
                m.conforms &= refined.conforms;
                m.refinement = Some(refined.clone());
            }
            m
        })
        .and_then(|m| {
            if frame.refinement.is_some() && m.refinement.is_none() {
                Err("composite refinement could not be fitted".into())
            } else {
                Ok(m)
            }
        })
    }

    /// Offset of the related actual mating envelope (or the minimum-material
    /// envelope at LMC) from true position in the measured frame, before any
    /// datum shift; cached per control. Frames are three planes (3-2-1) or a
    /// primary plane, a secondary axis and an optional tertiary plane.
    fn position_data(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
    ) -> Result<PositionData, String> {
        if let Some(cached) = self.positions.get(frame.id.as_str()) {
            return cached.clone();
        }
        let result = self.fit_position(frame, points);
        self.positions.insert(frame.id.clone(), result.clone());
        result
    }

    fn fit_position(
        &mut self,
        frame: &DrawingFeatureControlFrame,
        points: &[Vec3],
    ) -> Result<PositionData, String> {
        let limits = frame
            .size_limits
            .as_ref()
            .ok_or("position evaluation needs size limits to identify the mating envelope")?;
        let ResolvedDatum::Axis { origin, direction } = self.nominal(&frame.attachment.anchor)?
        else {
            return Err("position evaluation needs axis datum geometry".into());
        };
        let nominal = self.resolved_frame(&frame.datums, None)?;
        let axis_frame = nominal
            .datums
            .iter()
            .any(|d| matches!(d.geometry, ResolvedDatum::Axis { .. }));
        let nominal_frame = if axis_frame {
            nominal.plane_axis_frame()
        } else if frame.datums.len() == 3 {
            nominal.nominal_planar_321()
        } else {
            return Err(
                "position evaluation needs three plane datums, or a plane, an axis and an optional plane"
                    .into(),
            );
        }
        .map_err(|e| e.to_string())?;
        if length(cross(
            unit(direction).map_err(|e| e.to_string())?,
            nominal_frame.z_axis,
        )) > 1e-9
        {
            return Err(
                "position evaluation needs a feature axis normal to the primary datum".into(),
            );
        }
        let true_position = nominal_frame
            .coordinates_mm(origin)
            .map_err(|e| e.to_string())?;
        if axis_frame && frame.datums.len() == 2 && true_position.x.hypot(true_position.y) > 1e-9 {
            return Err(
                "position off a secondary datum axis needs a tertiary datum to fix rotation".into(),
            );
        }
        let simulators = self.simulators(&frame.datums)?;
        let measured_frame = if axis_frame {
            self.resolved_frame(&frame.datums, Some(&simulators))?
                .plane_axis_frame()
                .map_err(|e| e.to_string())?
        } else {
            self.measured_frame(&frame.datums)?
        };
        let shift_limit = simulators
            .iter()
            .map(|s| match s {
                Simulator::Axis(.., limit) => *limit,
                Simulator::Plane(..) => 0.0,
            })
            .sum();
        let projected = points
            .iter()
            .map(|&p| measured_frame.coordinates_mm(p).map(|c| [c.x, c.y]))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        // Internal features mate with the largest inscribed cylinder normal to
        // the primary datum; external ones with the smallest circumscribed one.
        let internal = limits.kind == FeatureOfSizeKind::Internal;
        let (mating, minimum_material) = if internal {
            (Fit::Inscribed, Fit::Circumscribed)
        } else {
            (Fit::Circumscribed, Fit::Inscribed)
        };
        let (lower, upper) = limits.bounds().map_err(|e| e.to_string())?;
        let (center, radius) = fit_circle(&projected, mating).map_err(|e| e.to_string())?;
        let size = 2.0 * radius;
        let (minimum_center, minimum_radius) =
            fit_circle(&projected, minimum_material).map_err(|e| e.to_string())?;
        let minimum_size = 2.0 * minimum_radius;
        let (center, bonus) = match frame.material {
            ToleranceMaterialCondition::Regardless => (center, 0.0),
            ToleranceMaterialCondition::Maximum => (
                center,
                if internal { size - lower } else { upper - size }.max(0.0),
            ),
            // Least material is located on the minimum material envelope axis.
            ToleranceMaterialCondition::Least => (
                minimum_center,
                if internal {
                    upper - minimum_size
                } else {
                    minimum_size - lower
                }
                .max(0.0),
            ),
        };
        let size_conforms = if internal {
            size >= lower && minimum_size <= upper
        } else {
            size <= upper && minimum_size >= lower
        };
        Ok(PositionData {
            offset: [center[0] - true_position.x, center[1] - true_position.y],
            true_position: [true_position.x, true_position.y],
            bonus,
            size,
            size_conforms,
            shift_limit,
        })
    }
}

#[derive(Clone, Debug)]
struct PositionData {
    offset: [f64; 2],
    /// True position in the frame, the lever arm for pattern rotation.
    true_position: [f64; 2],
    bonus: f64,
    size: f64,
    size_conforms: bool,
    /// Largest radial datum shift the frame's material boundaries allow.
    shift_limit: f64,
}

/// A member of a composite pattern: its offset from true position, its true
/// position relative to the pattern centroid, and its allowed zone radius.
#[derive(Clone, Copy, Debug)]
struct PatternMember {
    offset: [f64; 2],
    lever: [f64; 2],
    radius: f64,
}
impl PatternMember {
    /// Offset left after translating the true positions by `t` and turning
    /// them by `angle` about the centroid.
    fn residual(&self, t: [f64; 2], angle: f64) -> [f64; 2] {
        let (s, c) = angle.sin_cos();
        let turned = [
            c * self.lever[0] - s * self.lever[1],
            s * self.lever[0] + c * self.lever[1],
        ];
        [
            self.offset[0] - t[0] - (turned[0] - self.lever[0]),
            self.offset[1] - t[1] - (turned[1] - self.lever[1]),
        ]
    }
    fn excess(&self, t: [f64; 2], angle: f64) -> f64 {
        let r = self.residual(t, angle);
        r[0].hypot(r[1]) - self.radius
    }
}

/// Translation and rotation (radians, about the centroid; zero unless
/// `rotate`) minimizing the largest excess of any member over its zone, the
/// feature-relating fit of a composite pattern. An active set (the four worst
/// members, then the worst outside member) keeps each LP small; each LP is
/// relinearized in the angle and uses 256-gon zones. O(iterations · active ·
/// 256 · pivots + n).
fn pattern_fit(members: &[PatternMember], rotate: bool) -> ([f64; 2], f64) {
    if members.is_empty() {
        return ([0.0, 0.0], 0.0);
    }
    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by(|&a, &b| {
        members[b]
            .excess([0.0; 2], 0.0)
            .total_cmp(&members[a].excess([0.0; 2], 0.0))
    });
    let mut active: Vec<usize> = order.into_iter().take(4).collect();
    loop {
        let subset: Vec<PatternMember> = active.iter().map(|&i| members[i]).collect();
        let (t, angle) = pattern_lp(&subset, rotate);
        let active_worst = subset
            .iter()
            .map(|m| m.excess(t, angle))
            .fold(f64::NEG_INFINITY, f64::max);
        let (worst, value) = members
            .iter()
            .enumerate()
            .map(|(i, m)| (i, m.excess(t, angle)))
            .fold((0, f64::NEG_INFINITY), |a, b| if b.1 > a.1 { b } else { a });
        if value <= active_worst + 1e-12 || active.contains(&worst) {
            return (t, angle);
        }
        active.push(worst);
    }
}

fn pattern_lp(members: &[PatternMember], rotate: bool) -> ([f64; 2], f64) {
    const SIDES: usize = 256;
    let directions: Vec<[f64; 2]> = (0..SIDES)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / SIDES as f64;
            [a.cos(), a.sin()]
        })
        .collect();
    let (mut t, mut angle): ([f64; 2], f64) = ([0.0, 0.0], 0.0);
    for _ in 0..6 {
        // Variables: Δtx, Δty, Δangle, z; residual ≈ e − Δt − Δangle·q.
        let mut rows = Vec::with_capacity((members.len() * SIDES + 2) * 4);
        let mut bounds = Vec::with_capacity(members.len() * SIDES + 2);
        let (s, c) = angle.sin_cos();
        for m in members {
            let e = m.residual(t, angle);
            let turned = [
                c * m.lever[0] - s * m.lever[1],
                s * m.lever[0] + c * m.lever[1],
            ];
            let q = [-turned[1], turned[0]];
            for u in &directions {
                rows.extend([-u[0], -u[1], -(u[0] * q[0] + u[1] * q[1]), -1.0]);
                bounds.push(m.radius - u[0] * e[0] - u[1] * e[1]);
            }
        }
        let step = if rotate { 0.05 } else { 0.0 };
        for sign in [1.0, -1.0] {
            rows.extend([0.0, 0.0, sign, 0.0]);
            bounds.push(step);
        }
        let Ok(x) = fit::maximize(&[0.0, 0.0, 0.0, -1.0], &rows, &bounds) else {
            break;
        };
        t = [t[0] + x[0], t[1] + x[1]];
        angle += x[2];
        if x[2].abs() < 1e-12 {
            break;
        }
    }
    (t, angle)
}

/// One datum shift `t` (|t| ≤ `limit`) shared by controls under the
/// simultaneous requirement, minimizing the largest excess `|d − t| − r` of
/// each control's offset `d` over its allowed radius `r`. The optimum of this
/// 2-D min-max is fixed by at most three controls, so an active set starts
/// from the three worst at `t = 0` and adds the worst outside control until
/// none exceeds the active maximum: each step is a small LP. Circles are
/// circumscribed 256-gons (within 0.008% of optimal) and a 1e-6 weight on
/// |t|₁ picks the smallest of near-optimal shifts. Conformance is decided
/// exactly at the returned shift. O(iterations · (n + active · 256 · pivots)).
fn common_shift(controls: &[([f64; 2], f64)], limit: f64) -> [f64; 2] {
    if limit <= 0.0 || controls.is_empty() {
        return [0.0, 0.0];
    }
    let excess = |t: [f64; 2], &(d, r): &([f64; 2], f64)| (d[0] - t[0]).hypot(d[1] - t[1]) - r;
    let mut order: Vec<usize> = (0..controls.len()).collect();
    order.sort_by(|&a, &b| {
        excess([0.0; 2], &controls[b]).total_cmp(&excess([0.0; 2], &controls[a]))
    });
    let mut active: Vec<usize> = order.into_iter().take(3).collect();
    loop {
        let subset: Vec<_> = active.iter().map(|&i| controls[i]).collect();
        let t = shift_lp(&subset, limit);
        let active_worst = subset
            .iter()
            .map(|c| excess(t, c))
            .fold(f64::NEG_INFINITY, f64::max);
        let (worst, value) = controls
            .iter()
            .enumerate()
            .map(|(i, c)| (i, excess(t, c)))
            .fold((0, f64::NEG_INFINITY), |a, b| if b.1 > a.1 { b } else { a });
        if value <= active_worst + 1e-12 || active.contains(&worst) {
            return t;
        }
        active.push(worst);
    }
}

/// The min-max datum shift LP over `controls` (see [`common_shift`]).
fn shift_lp(controls: &[([f64; 2], f64)], limit: f64) -> [f64; 2] {
    const SIDES: usize = 256;
    let directions: Vec<[f64; 2]> = (0..SIDES)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / SIDES as f64;
            [a.cos(), a.sin()]
        })
        .collect();
    // Variables: tx, ty, z (largest excess), wx ≥ |tx|, wy ≥ |ty|.
    let mut rows = Vec::with_capacity(((controls.len() + 1) * SIDES + 4) * 5);
    let mut bounds = Vec::with_capacity((controls.len() + 1) * SIDES + 4);
    for &(d, r) in controls {
        for u in &directions {
            rows.extend([-u[0], -u[1], -1.0, 0.0, 0.0]);
            bounds.push(r - u[0] * d[0] - u[1] * d[1]);
        }
    }
    for u in &directions {
        rows.extend([u[0], u[1], 0.0, 0.0, 0.0]);
        bounds.push(limit);
    }
    for (axis, sign) in [(0, 1.0), (0, -1.0), (1, 1.0), (1, -1.0)] {
        let mut row = [0.0; 5];
        row[axis] = sign;
        row[3 + axis] = -1.0;
        rows.extend(row);
        bounds.push(0.0);
    }
    let Ok(x) = fit::maximize(&[0.0, 0.0, -1.0, -1e-6, -1e-6], &rows, &bounds) else {
        return [0.0, 0.0];
    };
    let length = x[0].hypot(x[1]);
    if length > limit && length > 0.0 {
        [x[0] * limit / length, x[1] * limit / length]
    } else {
        [x[0], x[1]]
    }
}

/// Largest minus smallest value; zero for an empty sequence.
fn spread(values: impl Iterator<Item = f64>) -> f64 {
    let (lo, hi) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    if lo.is_finite() { hi - lo } else { 0.0 }
}

/// Sorts readings by key and splits them where consecutive keys differ by
/// more than 1e-3 mm or 1e-6 of the key span.
fn elements(readings: &mut [(f64, f64)]) -> Vec<&[(f64, f64)]> {
    readings.sort_by(|a, b| a.0.total_cmp(&b.0));
    let span = readings.last().map_or(0.0, |r| r.0) - readings.first().map_or(0.0, |r| r.0);
    let gap = (1e-6 * span).max(1e-3);
    let mut out = Vec::new();
    let mut start = 0;
    for end in 1..=readings.len() {
        if end == readings.len() || readings[end].0 - readings[end - 1].0 > gap {
            out.push(&readings[start..end]);
            start = end;
        }
    }
    out
}

/// Two unit vectors completing a right-handed basis with unit `n`.
fn perpendiculars(n: Vec3) -> (Vec3, Vec3) {
    let seed = if n.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u = scale(cross(n, seed), 1.0 / length(cross(n, seed)));
    (u, cross(n, u))
}
